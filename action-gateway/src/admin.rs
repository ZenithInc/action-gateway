use action_gateway_v2::{
    rbac_manifest::{AccessPolicyRequest, ApiKeyRequest, PrincipalRequest},
    store::SourceRecord,
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AppState, audit, control_plane};

#[derive(Debug, Deserialize)]
pub struct SourceRequest {
    id: Option<String>,
    #[serde(alias = "sourceName")]
    source_name: String,
    #[serde(alias = "sourceType")]
    source_type: String,
    #[serde(alias = "displayName")]
    display_name: Option<String>,
    config: Option<Value>,
    credential: Option<Value>,
    #[serde(alias = "credentialVersion")]
    credential_version: Option<i64>,
    enabled: Option<bool>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/principals", get(list_principals).post(upsert_principal))
        .route("/api-keys", post(create_api_key))
        .route(
            "/access-policies",
            get(list_access_policies).post(upsert_access_policy),
        )
        .route("/sources", post(upsert_source))
}

async fn list_principals(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authenticate_admin(&state, &headers).await {
        return response;
    }
    let items = state
        .store
        .list_principals()
        .await
        .into_iter()
        .map(|principal| principal.to_json())
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(json!({ "items": items }))).into_response()
}

async fn upsert_principal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PrincipalRequest>,
) -> Response {
    let (auth, audit_context) = match authenticate_admin(&state, &headers).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let resource_id = request.id.clone();
    let result = state
        .store
        .upsert_principal(request)
        .await
        .map(|principal| principal.to_json());

    admin_write_response(
        &state,
        &audit_context,
        &auth,
        "admin.principal.upsert",
        "principal",
        &resource_id,
        result,
    )
    .await
}

async fn create_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ApiKeyRequest>,
) -> Response {
    let (auth, audit_context) = match authenticate_admin(&state, &headers).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let result = state
        .store
        .create_api_key(request)
        .await
        .map(|(key, token)| {
            json!({
                "id": key.id,
                "apiKey": token
            })
        });
    let resource_id = result
        .as_ref()
        .ok()
        .and_then(|value| value.get("id").and_then(Value::as_str))
        .unwrap_or("unknown")
        .to_string();

    admin_write_response(
        &state,
        &audit_context,
        &auth,
        "admin.api_key.create",
        "api_key",
        &resource_id,
        result,
    )
    .await
}

async fn list_access_policies(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authenticate_admin(&state, &headers).await {
        return response;
    }
    let items = state
        .store
        .list_access_policies()
        .await
        .into_iter()
        .map(|policy| policy.to_json())
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(json!({ "items": items }))).into_response()
}

async fn upsert_access_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AccessPolicyRequest>,
) -> Response {
    let (auth, audit_context) = match authenticate_admin(&state, &headers).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let resource_id = request.id.clone();
    let result = state
        .store
        .upsert_access_policy(request)
        .await
        .map(|policy| policy.to_json());

    admin_write_response(
        &state,
        &audit_context,
        &auth,
        "admin.access_policy.upsert",
        "access_policy",
        &resource_id,
        result,
    )
    .await
}

async fn upsert_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SourceRequest>,
) -> Response {
    let (auth, audit_context) = match authenticate_admin(&state, &headers).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let source_id = request
        .id
        .clone()
        .unwrap_or_else(|| format!("src_{}_{}", request.source_name, request.source_type));
    let record = SourceRecord {
        id: source_id.clone(),
        source_name: request.source_name,
        source_type: request.source_type,
        display_name: request.display_name,
        config: request.config.unwrap_or_else(|| json!({})),
        credential: request.credential,
        credential_version: request.credential_version,
        enabled: request.enabled.unwrap_or(true),
        created_at: String::new(),
        updated_at: String::new(),
    };
    let result = state
        .store
        .upsert_source(record)
        .await
        .map(|source| source.to_json());

    admin_write_response(
        &state,
        &audit_context,
        &auth,
        "admin.source.upsert",
        "source",
        &source_id,
        result,
    )
    .await
}

async fn authenticate_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(control_plane::AuthContext, audit::AuditContext), Response> {
    let base_context = crate::audit_context_from_headers(headers);
    let auth = match control_plane::authenticate(&state.store, headers, &state.auth_config).await {
        Ok(auth) => auth,
        Err(error) => {
            let _ = audit::record_authentication_failure(&state.store, &base_context, &error).await;
            return Err((StatusCode::UNAUTHORIZED, "unauthorized").into_response());
        }
    };
    if !auth.unrestricted
        && !auth
            .scopes
            .get("admin")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err((StatusCode::FORBIDDEN, "forbidden").into_response());
    }
    let audit_context = base_context.with_auth(&auth);
    let _ =
        audit::record_authentication_event(&state.store, &audit_context, "succeeded", None).await;

    Ok((auth, audit_context))
}

async fn admin_write_response(
    state: &AppState,
    context: &audit::AuditContext,
    auth: &control_plane::AuthContext,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    result: Result<Value, String>,
) -> Response {
    match result {
        Ok(value) => {
            let _ = audit::record_admin_config_event(
                &state.store,
                context,
                auth,
                action,
                resource_type,
                resource_id,
                "succeeded",
                None,
            )
            .await;
            (StatusCode::OK, Json(value)).into_response()
        }
        Err(message) => {
            let _ = audit::record_admin_config_event(
                &state.store,
                context,
                auth,
                action,
                resource_type,
                resource_id,
                "failed",
                Some(&message),
            )
            .await;
            json_error(StatusCode::BAD_REQUEST, &message)
        }
    }
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "message": message }))).into_response()
}
