use action_gateway_v2::store::FileStore;
use serde_json::{Value, json};

use crate::{actions, control_plane};

const PROTOCOL_VERSION: &str = "2025-11-25";

#[cfg(test)]
pub async fn handle_json_rpc(
    store: &FileStore,
    redis: &redis::Client,
    body: &str,
) -> Option<Value> {
    handle_json_rpc_with_auth(
        store,
        redis,
        &control_plane::AuthContext::legacy_admin(),
        body,
    )
    .await
}

pub async fn handle_json_rpc_with_auth(
    store: &FileStore,
    redis: &redis::Client,
    auth: &control_plane::AuthContext,
    body: &str,
) -> Option<Value> {
    let request = match serde_json::from_str::<Value>(body) {
        Ok(request) => request,
        Err(_) => {
            return Some(error_response(Value::Null, -32700, "parse error"));
        }
    };

    handle_json_rpc_value(store, redis, auth, &request).await
}

async fn handle_json_rpc_value(
    store: &FileStore,
    redis: &redis::Client,
    auth: &control_plane::AuthContext,
    request: &Value,
) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = match request.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => {
            return Some(error_response(
                id.unwrap_or(Value::Null),
                -32600,
                "invalid request",
            ));
        }
    };

    let result = match method {
        "initialize" => Ok(initialize_result()),
        "notifications/initialized" => return None,
        "tools/list" => actions::list_tools_for_auth(store, auth)
            .await
            .map_err(|_| (-32603, "failed to list authorized tools")),
        "tools/call" => actions::call_tool_for_auth(store, redis, auth, request).await,
        "ping" => Ok(json!({})),
        _ => Err((-32601, "method not found")),
    };

    match (id, result) {
        (Some(id), Ok(result)) => Some(success_response(id, result)),
        (Some(id), Err((code, message))) => Some(error_response(id, code, message)),
        (None, _) => None,
    }
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": "action-skills-mcp-gateway",
            "title": "Action Skills MCP Gateway",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Gateway exposing operational actions as MCP tools."
        },
        "instructions": "Use tools/list to discover the actions visible to the authenticated Gateway API key and tools/call to invoke them. Source-backed tools accept an optional source_name. Data table queries use a registered MySQL source and require table policy. Redis key queries are read-only and require key policy. Kubernetes access is structured-tool-first and constrained by source, namespace, resource, and action policy; raw kubectl is hidden unless KUBERNETES_ENABLE_RAW_KUBECTL=true and should be reserved for break-glass diagnostics. SLS log queries use registered Alibaba Cloud Simple Log Service sources and require Logstore policy."
    })
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_store() -> FileStore {
        let path = std::env::temp_dir().join(format!(
            "action-gateway-test-{}.json",
            uuid::Uuid::new_v4().simple()
        ));
        FileStore::load(path).await.expect("test store should load")
    }

    fn test_redis() -> redis::Client {
        redis::Client::open("redis://127.0.0.1:6379/").expect("test redis client should be created")
    }

    #[tokio::test]
    async fn initializes_mcp_gateway() {
        let store = test_store().await;
        let redis = test_redis();
        let response = handle_json_rpc(
            &store,
            &redis,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#,
        )
        .await
        .expect("initialize should respond");

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(
            response["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
    }

    #[tokio::test]
    async fn ignores_initialized_notification() {
        let store = test_store().await;
        let redis = test_redis();
        let response = handle_json_rpc(
            &store,
            &redis,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        )
        .await;

        assert!(response.is_none());
    }

    #[tokio::test]
    async fn lists_tools() {
        let store = test_store().await;
        let redis = test_redis();
        let response = handle_json_rpc(
            &store,
            &redis,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        )
        .await
        .expect("tools/list should respond");

        let tool_names = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            tool_names.len(),
            actions::list_tools()["tools"].as_array().unwrap().len()
        );
        assert_eq!(
            response["result"]["tools"][0]["name"],
            actions::TOOL_QUERY_TABLE_DATA
        );
        assert!(tool_names.contains(&actions::TOOL_LIST_KUBERNETES_RESOURCES));
        assert!(tool_names.contains(&actions::TOOL_GET_KUBERNETES_RESOURCE));
        assert!(tool_names.contains(&actions::TOOL_KUBERNETES_ROLLOUT_STATUS));
    }

    #[tokio::test]
    async fn validates_sls_log_tool_arguments_without_touching_network() {
        let store = test_store().await;
        let redis = test_redis();
        let response = handle_json_rpc(
            &store,
            &redis,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"logs.query_sls_logs","arguments":{"line":20}}}"#,
        )
        .await
        .expect("tools/call should respond");

        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["structuredContent"]["status"],
            "invalid_arguments"
        );
        assert_eq!(
            response["result"]["structuredContent"]["action"],
            actions::TOOL_QUERY_SLS_LOGS
        );
    }

    #[tokio::test]
    async fn returns_tool_error_for_invalid_arguments() {
        let store = test_store().await;
        let redis = test_redis();
        let response = handle_json_rpc(
            &store,
            &redis,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"data.query_table","arguments":{}}}"#,
        )
        .await
        .expect("tools/call should respond");

        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["structuredContent"]["status"],
            "invalid_arguments"
        );
    }

    #[tokio::test]
    async fn reports_parse_error() {
        let store = test_store().await;
        let redis = test_redis();
        let response = handle_json_rpc(&store, &redis, "not json")
            .await
            .expect("parse error should respond");

        assert_eq!(
            response,
            json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}})
        );
    }

    #[tokio::test]
    async fn reports_unknown_method() {
        let store = test_store().await;
        let redis = test_redis();
        let response = handle_json_rpc(
            &store,
            &redis,
            r#"{"jsonrpc":"2.0","id":5,"method":"missing"}"#,
        )
        .await
        .expect("request should respond with error");

        assert_eq!(
            response,
            json!({"jsonrpc":"2.0","id":5,"error":{"code":-32601,"message":"method not found"}})
        );
    }
}
