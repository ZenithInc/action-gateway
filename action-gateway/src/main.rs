mod actions;
mod admin;
mod audit;
mod control_plane;
mod mcp;
mod sls;

use std::{convert::Infallible, net::SocketAddr};

use action_gateway_v2::store::FileStore;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::stream::{self, StreamExt};

#[derive(Clone)]
struct AppState {
    auth_config: control_plane::AuthConfig,
    store: FileStore,
    redis: redis::Client,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = std::env::var("RPC_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let token = legacy_token_from_env();
    let loopback_bind = is_loopback_bind_addr(&bind_addr);
    let legacy_token_allowed = loopback_bind || env_flag("GATEWAY_ALLOW_LEGACY_RPC_TOKEN");
    if token.is_some() && !legacy_token_allowed {
        return Err(
            "RPC_TOKEN/ACTION_GATEWAY_MCP_TOKEN is local/demo compatibility only; set GATEWAY_ALLOW_LEGACY_RPC_TOKEN=true for an explicit demo deployment or provision Gateway API keys in the file store for production"
                .into(),
        );
    }
    let auth_config = control_plane::AuthConfig {
        legacy_token: token.clone(),
        legacy_token_allowed,
        anonymous_local_allowed: loopback_bind && token.is_none(),
    };
    let store_path =
        std::env::var("GATEWAY_STORE_FILE").unwrap_or_else(|_| "gateway-store.json".to_string());
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());
    log_startup_config(
        &bind_addr,
        token.is_some(),
        legacy_token_allowed,
        auth_config.anonymous_local_allowed,
        &store_path,
    );
    let store = FileStore::load(&store_path).await?;
    let redis = redis::Client::open(redis_url).inspect_err(|error| {
        eprintln!(
            "{}",
            serde_json::json!({
                "event": "dependency_connection_failed",
                "dependency": "redis",
                "message": error.to_string()
            })
        );
    })?;
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    eprintln!(
        "Action MCP Gateway listening on http://{bind_addr}/mcp and http://{bind_addr}/admin"
    );

    axum::serve(
        listener,
        app(AppState {
            auth_config,
            store,
            redis,
        }),
    )
    .await?;

    Ok(())
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/", post(handle_mcp).get(handle_mcp_stream))
        .route("/healthz", get(healthz))
        .route("/mcp", post(handle_mcp).get(handle_mcp_stream))
        .route("/mcp/", post(handle_mcp).get(handle_mcp_stream))
        .route("/rpc", post(handle_mcp).get(handle_mcp_stream))
        .route("/rpc/", post(handle_mcp).get(handle_mcp_stream))
        .nest("/admin", admin::router())
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn handle_mcp_stream(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match control_plane::authenticate(&state.store, &headers, &state.auth_config).await {
        Ok(_) => Sse::new(
            stream::once(async { Ok(Event::default().comment("connected")) })
                .chain(stream::pending::<Result<Event, Infallible>>()),
        )
        .keep_alive(KeepAlive::default())
        .into_response(),
        Err(error) => {
            let status = match error.kind {
                control_plane::AuthErrorKind::StoreUnavailable => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::UNAUTHORIZED,
            };
            (
                status,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"message":"unauthorized"}"#,
            )
                .into_response()
        }
    }
}

async fn handle_mcp(State(state): State<AppState>, headers: HeaderMap, body: String) -> Response {
    let base_audit_context = audit_context_from_headers(&headers);
    let auth_context = match control_plane::authenticate(&state.store, &headers, &state.auth_config)
        .await
    {
        Ok(auth_context) => {
            let audit_context = base_audit_context.clone().with_auth(&auth_context);
            if let Err(error) =
                audit::record_authentication_event(&state.store, &audit_context, "succeeded", None)
                    .await
            {
                eprintln!("failed to record authentication audit event: {error}");
            }
            auth_context
        }
        Err(error) => {
            if let Err(audit_error) =
                audit::record_authentication_failure(&state.store, &base_audit_context, &error)
                    .await
            {
                eprintln!("failed to record authentication audit event: {audit_error}");
            }
            let status = match error.kind {
                control_plane::AuthErrorKind::StoreUnavailable => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::UNAUTHORIZED,
            };
            return (status, "unauthorized").into_response();
        }
    };

    let response =
        mcp::handle_json_rpc_with_auth(&state.store, &state.redis, &auth_context, &body).await;
    if let Some(response) = response.as_ref() {
        log_mcp_tool_call_status(&body, response);
    }
    let audit_context = base_audit_context.with_auth(&auth_context);
    if let Err(error) =
        audit::record_mcp_tool_call(&state.store, &audit_context, &body, response.as_ref()).await
    {
        eprintln!("failed to record approval audit event: {error}");
    }

    match response {
        Some(response) => (StatusCode::OK, Json(response)).into_response(),
        None => (
            StatusCode::ACCEPTED,
            [(header::CONTENT_TYPE, "application/json")],
            "",
        )
            .into_response(),
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn legacy_token_from_env() -> Option<String> {
    non_empty_env("RPC_TOKEN").or_else(|| non_empty_env("ACTION_GATEWAY_MCP_TOKEN"))
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        )
    })
}

fn is_loopback_bind_addr(bind_addr: &str) -> bool {
    if let Ok(socket_addr) = bind_addr.parse::<SocketAddr>() {
        return socket_addr.ip().is_loopback();
    }

    let host = bind_addr
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(bind_addr)
        .trim_matches(['[', ']']);
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

fn log_startup_config(
    bind_addr: &str,
    legacy_token_configured: bool,
    legacy_token_allowed: bool,
    anonymous_local_allowed: bool,
    store_path: &str,
) {
    eprintln!(
        "{}",
        serde_json::json!({
            "event": "startup_config",
            "rpcBindAddr": bind_addr,
            "authMode": "gateway_api_key",
            "legacyRpcTokenConfigured": legacy_token_configured,
            "legacyRpcTokenAllowed": legacy_token_allowed,
            "anonymousLocalAllowed": anonymous_local_allowed,
            "storeFile": store_path,
            "rawKubectlEnabled": actions::raw_kubectl_enabled()
        })
    );
}

fn log_mcp_tool_call_status(request_body: &str, response: &serde_json::Value) {
    let Ok(request) = serde_json::from_str::<serde_json::Value>(request_body) else {
        return;
    };
    if request.get("method").and_then(serde_json::Value::as_str) != Some("tools/call") {
        return;
    }

    let action = request
        .pointer("/params/name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let structured = response.pointer("/result/structuredContent");
    let status = structured
        .and_then(|structured| structured.get("status"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| response.get("error").map(|_| "protocol_error"))
        .unwrap_or("unknown");
    let is_error = response
        .pointer("/result/isError")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| response.get("error").is_some());

    eprintln!(
        "{}",
        serde_json::json!({
            "event": "tool_call",
            "action": action,
            "status": status,
            "isError": is_error
        })
    );
}

fn audit_context_from_headers(headers: &HeaderMap) -> audit::AuditContext {
    audit::AuditContext::new(
        optional_header(headers, "x-request-id", 128),
        optional_header(headers, "x-approval-id", 64),
        optional_header(headers, "x-action-request-id", 64),
        optional_header(headers, "x-actor-id", 128)
            .or_else(|| optional_header(headers, "x-user-id", 128)),
        optional_header(headers, "x-actor-role", 128),
        forwarded_source_ip(headers),
        optional_header(headers, "user-agent", 512),
        optional_header(headers, "x-service-name", 255),
    )
}

fn optional_header(headers: &HeaderMap, name: &str, max_chars: usize) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?.trim();
    if value.is_empty() {
        return None;
    }

    Some(value.chars().take(max_chars).collect())
}

fn forwarded_source_ip(headers: &HeaderMap) -> Option<String> {
    optional_header(headers, "x-forwarded-for", 255)
        .and_then(|value| value.split(',').next().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
        .or_else(|| optional_header(headers, "x-real-ip", 255))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::AUTHORIZATION;

    #[test]
    fn detects_env_flags() {
        assert!(matches!(
            ["true", "1", "yes"]
                .map(|value| value.trim().to_ascii_lowercase())
                .as_slice(),
            [_, _, _]
        ));
    }

    #[test]
    fn extracts_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer secret".parse().unwrap());

        assert_eq!(control_plane::bearer_token(&headers), Some("secret"));
    }

    #[test]
    fn detects_loopback_bind_addresses() {
        assert!(is_loopback_bind_addr("127.0.0.1:8080"));
        assert!(is_loopback_bind_addr("localhost:8080"));
        assert!(is_loopback_bind_addr("[::1]:8080"));
        assert!(!is_loopback_bind_addr("0.0.0.0:8080"));
        assert!(!is_loopback_bind_addr("10.0.0.12:8080"));
    }
}
