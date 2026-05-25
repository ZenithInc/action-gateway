use std::{
    collections::{BTreeSet, HashSet},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use action_gateway_v2::store::FileStore;
use redis::Client as RedisClient;
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{
    MySql, MySqlPool, QueryBuilder, Row,
    mysql::{MySqlPoolOptions, MySqlRow},
    types::Json,
};
use tokio::{
    fs,
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    time,
};

use crate::{audit, control_plane};

pub const TOOL_QUERY_TABLE_DATA: &str = "data.query_table";
pub const TOOL_QUERY_REDIS_KEY: &str = "redis.query_key";
pub const TOOL_LIST_KUBERNETES_RESOURCES: &str = "kubernetes.list_resources";
pub const TOOL_GET_KUBERNETES_RESOURCE: &str = "kubernetes.get_resource";
pub const TOOL_KUBERNETES_ROLLOUT_STATUS: &str = "kubernetes.rollout_status";
pub const TOOL_RUN_KUBECTL_READ: &str = "kubernetes.kubectl_read";
pub const TOOL_QUERY_POD_LOGS: &str = "kubernetes.query_pod_logs";
pub const TOOL_QUERY_APP_LOGS: &str = "logs.query_app_logs";

pub fn list_tools() -> Value {
    let mut tools = vec![
        json!({
                "name": TOOL_QUERY_TABLE_DATA,
                "title": "Query Table Data",
                "description": "Query rows from an allowlisted MySQL table after passing an EXPLAIN gate and applying configured field masking.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source_name": {
                            "type": "string",
                            "description": "Optional logical source name in the allowlist.",
                            "default": "default"
                        },
                        "table_name": {
                            "type": "string",
                            "description": "Logical table name to query."
                        },
                        "columns": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional list of columns to return."
                        },
                        "filters": {
                            "type": "object",
                            "description": "Optional equality filters keyed by column name."
                        },
                        "order_by": {
                            "type": "array",
                            "maxItems": 3,
                            "description": "Optional ordered sort keys. Columns must be allowlisted.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "column": {
                                        "type": "string",
                                        "description": "Column to sort by."
                                    },
                                    "direction": {
                                        "type": "string",
                                        "enum": ["asc", "desc"],
                                        "default": "asc"
                                    }
                                },
                                "required": ["column"],
                                "additionalProperties": false
                            }
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 1000,
                            "default": 100
                        }
                    },
                    "required": ["table_name"],
                    "additionalProperties": false
                }
        }),
        json!({
                "name": TOOL_QUERY_REDIS_KEY,
                "title": "Query Redis Key",
                "description": "Read a Redis key after matching it against the configured key allowlist. This tool only runs read commands.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source_name": {
                            "type": "string",
                            "description": "Optional logical Redis source name in the allowlist.",
                            "default": "default"
                        },
                        "key": {
                            "type": "string",
                            "description": "Redis key to query."
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 1000,
                            "description": "Maximum collection members or entries to return."
                        }
                    },
                    "required": ["key"],
                    "additionalProperties": false
                }
        }),
        json!({
                "name": TOOL_LIST_KUBERNETES_RESOURCES,
                "title": "List Kubernetes Resources",
                "description": "List allowlisted Kubernetes resources in one namespace. Returns structured summaries, not raw YAML or JSON.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source_name": {
                            "type": "string",
                            "description": "Optional logical Kubernetes source name in the allowlist.",
                            "default": "default"
                        },
                        "namespace": {
                            "type": "string",
                            "description": "Kubernetes namespace."
                        },
                        "resource": {
                            "type": "string",
                            "description": "Kubernetes resource type, such as pods or deployments."
                        },
                        "label_selector": {
                            "type": "string",
                            "description": "Optional label selector, such as app=api,tier!=debug."
                        },
                        "field_selector": {
                            "type": "string",
                            "description": "Optional field selector, such as status.phase=Running."
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 1000,
                            "description": "Maximum resources to return, capped by the allowlist."
                        }
                    },
                    "required": ["namespace", "resource"],
                    "additionalProperties": false
                }
        }),
        json!({
                "name": TOOL_GET_KUBERNETES_RESOURCE,
                "title": "Get Kubernetes Resource",
                "description": "Get one allowlisted Kubernetes resource and return a redacted, type-aware status summary.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source_name": {
                            "type": "string",
                            "description": "Optional logical Kubernetes source name in the allowlist.",
                            "default": "default"
                        },
                        "namespace": {
                            "type": "string",
                            "description": "Kubernetes namespace."
                        },
                        "resource": {
                            "type": "string",
                            "description": "Kubernetes resource type, such as pods or deployments."
                        },
                        "name": {
                            "type": "string",
                            "description": "Resource name."
                        }
                    },
                    "required": ["namespace", "resource", "name"],
                    "additionalProperties": false
                }
        }),
        json!({
                "name": TOOL_KUBERNETES_ROLLOUT_STATUS,
                "title": "Kubernetes Rollout Status",
                "description": "Query rollout status or history for allowlisted deployments, statefulsets, or daemonsets.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source_name": {
                            "type": "string",
                            "description": "Optional logical Kubernetes source name in the allowlist.",
                            "default": "default"
                        },
                        "namespace": {
                            "type": "string",
                            "description": "Kubernetes namespace."
                        },
                        "resource": {
                            "type": "string",
                            "description": "deployments, statefulsets, or daemonsets."
                        },
                        "name": {
                            "type": "string",
                            "description": "Workload name."
                        },
                        "action": {
                            "type": "string",
                            "enum": ["status", "history"],
                            "default": "status",
                            "description": "Rollout query type."
                        },
                        "revision": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 1000000,
                            "description": "Optional revision for rollout history."
                        }
                    },
                    "required": ["namespace", "resource", "name"],
                    "additionalProperties": false
                }
        }),
        json!({
                "name": TOOL_QUERY_POD_LOGS,
                "title": "Query Pod Logs",
                "description": "Query allowlisted Kubernetes Pod logs through kubectl logs. Tail lines and output bytes are capped by policy.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source_name": {
                            "type": "string",
                            "description": "Optional logical Kubernetes source name in the allowlist.",
                            "default": "default"
                        },
                        "namespace": {
                            "type": "string",
                            "description": "Kubernetes namespace."
                        },
                        "pod_name": {
                            "type": "string",
                            "description": "Pod name."
                        },
                        "container": {
                            "type": "string",
                            "description": "Optional container name."
                        },
                        "since": {
                            "type": "string",
                            "description": "Optional time window, such as 15m or 1h."
                        },
                        "previous": {
                            "type": "boolean",
                            "default": false,
                            "description": "Return logs for the previous terminated container instance."
                        },
                        "timestamps": {
                            "type": "boolean",
                            "default": false,
                            "description": "Include timestamps in log lines."
                        },
                        "tail_lines": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 5000,
                            "default": 200
                        },
                        "timeout_seconds": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 60,
                            "default": 10
                        },
                        "max_output_bytes": {
                            "type": "integer",
                            "minimum": 1024,
                            "maximum": 1048576,
                            "default": 65536
                        }
                    },
                    "required": ["namespace", "pod_name"],
                    "additionalProperties": false
                }
        }),
        json!({
                "name": TOOL_QUERY_APP_LOGS,
                "title": "Query Application Logs",
                "description": "Query bounded application log summaries from Redis app log indexes by app, environment, trace id, keyword, or recent time window.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source_name": {
                            "type": "string",
                            "description": "Optional logical Redis log source name.",
                            "default": "default"
                        },
                        "app_name": {
                            "type": "string",
                            "description": "Application/service name."
                        },
                        "environment": {
                            "type": "string",
                            "description": "Optional runtime environment, such as prod or staging."
                        },
                        "trace_id": {
                            "type": "string",
                            "description": "Optional trace id."
                        },
                        "keyword": {
                            "type": "string",
                            "description": "Optional keyword to search for."
                        },
                        "since": {
                            "type": "string",
                            "description": "Optional time window, such as 15m or 1h."
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 200,
                            "default": 50
                        }
                    },
                    "required": ["app_name"],
                    "additionalProperties": false
                }
        }),
        audit::tool_definition(),
    ];

    if raw_kubectl_enabled() {
        tools.insert(
            5,
            json!({
                    "name": TOOL_RUN_KUBECTL_READ,
                    "title": "Run Raw Diagnostic Kubectl",
                    "description": "Advanced diagnostic escape hatch. Disabled unless KUBERNETES_ENABLE_RAW_KUBECTL=true and still constrained by Kubernetes allowlist policy.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "source_name": {
                                "type": "string",
                                "description": "Optional logical Kubernetes source name in the allowlist.",
                                "default": "default"
                            },
                            "args": {
                                "type": "array",
                                "items": { "type": "string" },
                                "minItems": 1,
                                "description": "Arguments after kubectl. Only limited diagnostics are allowed."
                            },
                            "timeout_seconds": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 60,
                                "default": 10
                            },
                            "max_output_bytes": {
                                "type": "integer",
                                "minimum": 1024,
                                "maximum": 1048576,
                                "default": 65536,
                                "description": "Maximum bytes captured per output stream, capped by policy for resource commands."
                            }
                        },
                        "required": ["args"],
                        "additionalProperties": false
                    }
            }),
        );
    }

    json!({
        "tools": tools
    })
}

pub async fn list_tools_for_auth(
    store: &FileStore,
    auth: &control_plane::AuthContext,
) -> Result<Value, String> {
    let tools = list_tools();
    if auth.unrestricted {
        return Ok(tools);
    }

    let allowed_tool_names = control_plane::allowed_tool_names(store, auth).await?;
    let Some(allowed_tool_names) = allowed_tool_names else {
        return Ok(tools);
    };
    let filtered = tools
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| allowed_tool_names.contains(name))
        })
        .cloned()
        .collect::<Vec<_>>();

    Ok(json!({ "tools": filtered }))
}

#[cfg(test)]
pub async fn call_tool(
    store: &FileStore,
    redis: &RedisClient,
    request: &Value,
) -> Result<Value, (i32, &'static str)> {
    call_tool_for_auth(
        store,
        redis,
        &control_plane::AuthContext::legacy_admin(),
        request,
    )
    .await
}

pub async fn call_tool_for_auth(
    store: &FileStore,
    redis: &RedisClient,
    auth: &control_plane::AuthContext,
    request: &Value,
) -> Result<Value, (i32, &'static str)> {
    let params = request.get("params").ok_or((-32602, "missing params"))?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "missing params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if !arguments.is_object() {
        return Err((-32602, "params.arguments must be an object"));
    }

    let (arguments, authorization) =
        match authorize_tool_call(store, auth, name, arguments.clone()).await {
            Ok(authorization) => authorization,
            Err(message) => return Ok(tool_argument_error(name, &message, &arguments)),
        };
    if let Some((scope, decision)) = &authorization
        && !decision.allowed
    {
        return Ok(tool_error_result_with_authorization(
            name,
            "not_allowed",
            &decision.reason,
            &arguments,
            scope,
            decision,
        ));
    }

    let result = match name {
        TOOL_QUERY_TABLE_DATA => query_table_data(store, &arguments).await,
        TOOL_QUERY_REDIS_KEY => query_redis_key(store, redis, &arguments).await,
        TOOL_LIST_KUBERNETES_RESOURCES => list_kubernetes_resources(store, &arguments).await,
        TOOL_GET_KUBERNETES_RESOURCE => get_kubernetes_resource(store, &arguments).await,
        TOOL_KUBERNETES_ROLLOUT_STATUS => kubernetes_rollout_status(store, &arguments).await,
        TOOL_RUN_KUBECTL_READ => run_kubectl_read(store, &arguments).await,
        TOOL_QUERY_POD_LOGS => query_pod_logs(store, &arguments).await,
        TOOL_QUERY_APP_LOGS => query_app_logs(store, redis, &arguments).await,
        audit::TOOL_QUERY_APPROVAL_AUDIT_EVENTS => {
            audit::query_approval_audit_events(store, &arguments).await
        }
        _ => return Err((-32602, "unknown tool")),
    };

    Ok(match authorization {
        Some((scope, decision)) => with_authorization_summary(result, &scope, &decision),
        None => result,
    })
}

async fn authorize_tool_call(
    store: &FileStore,
    auth: &control_plane::AuthContext,
    tool_name: &str,
    arguments: Value,
) -> Result<
    (
        Value,
        Option<(
            control_plane::ToolAuthorizationScope,
            control_plane::AccessDecision,
        )>,
    ),
    String,
> {
    let Some(scope) = tool_authorization_scope(tool_name, &arguments, auth)? else {
        return Ok((arguments, None));
    };
    let arguments = control_plane::arguments_with_source_ref(&arguments, &scope.source);
    let decision = control_plane::authorize_tool(store, auth, &scope).await?;

    Ok((arguments, Some((scope, decision))))
}

fn tool_authorization_scope(
    tool_name: &str,
    arguments: &Value,
    auth: &control_plane::AuthContext,
) -> Result<Option<control_plane::ToolAuthorizationScope>, String> {
    let source = control_plane::source_ref_from_arguments(arguments, auth)?;
    let (action_name, resource_type, resource_name) = match tool_name {
        TOOL_QUERY_TABLE_DATA => (
            "select".to_string(),
            Some("table".to_string()),
            arguments
                .get("table_name")
                .and_then(Value::as_str)
                .map(str::to_string),
        ),
        TOOL_QUERY_REDIS_KEY => (
            "get".to_string(),
            Some("redis_key".to_string()),
            arguments
                .get("key")
                .and_then(Value::as_str)
                .map(str::to_string),
        ),
        TOOL_LIST_KUBERNETES_RESOURCES => {
            let resource = arguments
                .get("resource")
                .and_then(Value::as_str)
                .map(normalize_kubernetes_resource_name_lossy);
            (
                "list".to_string(),
                Some("kubernetes".to_string()),
                kubernetes_resource_scope_name(
                    arguments.get("namespace").and_then(Value::as_str),
                    resource.as_deref(),
                    Some("*"),
                ),
            )
        }
        TOOL_GET_KUBERNETES_RESOURCE => {
            let resource = arguments
                .get("resource")
                .and_then(Value::as_str)
                .map(normalize_kubernetes_resource_name_lossy);
            (
                "get".to_string(),
                Some("kubernetes".to_string()),
                kubernetes_resource_scope_name(
                    arguments.get("namespace").and_then(Value::as_str),
                    resource.as_deref(),
                    arguments.get("name").and_then(Value::as_str),
                ),
            )
        }
        TOOL_KUBERNETES_ROLLOUT_STATUS => {
            let resource = arguments
                .get("resource")
                .and_then(Value::as_str)
                .map(normalize_kubernetes_resource_name_lossy);
            let action = match arguments.get("action").and_then(Value::as_str) {
                Some("history") => "rollout_history",
                _ => "rollout_status",
            };
            (
                action.to_string(),
                Some("kubernetes".to_string()),
                kubernetes_resource_scope_name(
                    arguments.get("namespace").and_then(Value::as_str),
                    resource.as_deref(),
                    arguments.get("name").and_then(Value::as_str),
                ),
            )
        }
        TOOL_QUERY_POD_LOGS => (
            "logs".to_string(),
            Some("kubernetes".to_string()),
            kubernetes_resource_scope_name(
                arguments.get("namespace").and_then(Value::as_str),
                Some("pods"),
                arguments.get("pod_name").and_then(Value::as_str),
            ),
        ),
        TOOL_RUN_KUBECTL_READ => (
            "raw_read".to_string(),
            Some("kubernetes".to_string()),
            arguments.get("args").and_then(Value::as_array).map(|args| {
                args.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
        ),
        TOOL_QUERY_APP_LOGS => (
            "query".to_string(),
            Some("app_logs".to_string()),
            arguments
                .get("app_name")
                .and_then(Value::as_str)
                .map(str::to_string),
        ),
        audit::TOOL_QUERY_APPROVAL_AUDIT_EVENTS => (
            "query".to_string(),
            Some("audit_events".to_string()),
            Some("approval_audit_events".to_string()),
        ),
        _ => return Ok(None),
    };

    Ok(Some(control_plane::ToolAuthorizationScope {
        source,
        tool_name: tool_name.to_string(),
        action_name,
        resource_type,
        resource_name,
    }))
}

fn kubernetes_resource_scope_name(
    namespace: Option<&str>,
    resource: Option<&str>,
    name: Option<&str>,
) -> Option<String> {
    Some(format!(
        "{}/{}/{}",
        namespace?,
        resource?,
        name.unwrap_or("*")
    ))
}

fn normalize_kubernetes_resource_name_lossy(resource: &str) -> String {
    match resource {
        "po" | "pod" | "pods" => "pods",
        "deploy" | "deploys" | "deployment" | "deployments" => "deployments",
        "sts" | "statefulset" | "statefulsets" => "statefulsets",
        "ds" | "daemonset" | "daemonsets" => "daemonsets",
        "svc" | "service" | "services" => "services",
        "ing" | "ingress" | "ingresses" => "ingresses",
        "job" | "jobs" => "jobs",
        "pvc" | "pvcs" | "persistentvolumeclaim" | "persistentvolumeclaims" => {
            "persistentvolumeclaims"
        }
        "ev" | "event" | "events" => "events",
        other => other,
    }
    .to_string()
}

async fn query_table_data(store: &FileStore, arguments: &Value) -> Value {
    if missing_string(arguments, "table_name") {
        return tool_argument_error(
            TOOL_QUERY_TABLE_DATA,
            "missing required argument: table_name",
            arguments,
        );
    }

    let source_ref = match source_ref_from_tool_arguments(arguments) {
        Ok(source_ref) => source_ref,
        Err(message) => return tool_argument_error(TOOL_QUERY_TABLE_DATA, &message, arguments),
    };
    let table_name = arguments
        .get("table_name")
        .and_then(Value::as_str)
        .expect("table_name was checked above");
    let table_path = match parse_table_path(table_name) {
        Ok(path) => path,
        Err(message) => return tool_argument_error(TOOL_QUERY_TABLE_DATA, &message, arguments),
    };
    let allowlist = match load_allowed_table(store, &source_ref, table_name).await {
        Some(allowlist) => allowlist,
        None => {
            return tool_error_result(
                TOOL_QUERY_TABLE_DATA,
                "not_allowed",
                "table is not allowlisted",
                arguments,
            );
        }
    };
    let (source_db, credential_version) = match mysql_pool_for_source(store, &source_ref).await {
        Ok((source_db, credential_version)) => (source_db, credential_version),
        Err(message) => {
            return tool_error_result(TOOL_QUERY_TABLE_DATA, "query_failed", &message, arguments);
        }
    };
    let actual_columns = match load_table_columns(&source_db, &table_path).await {
        Ok(columns) if !columns.is_empty() => columns,
        Ok(_) => {
            return tool_error_result(
                TOOL_QUERY_TABLE_DATA,
                "query_failed",
                "table does not exist or has no columns",
                arguments,
            );
        }
        Err(error) => {
            return tool_error_result(
                TOOL_QUERY_TABLE_DATA,
                "query_failed",
                &format!("failed to inspect table columns: {error}"),
                arguments,
            );
        }
    };
    let allowed_columns = match normalize_columns(allowlist.columns, "allowlist column") {
        Ok(columns) => columns,
        Err(message) => {
            return tool_error_result(TOOL_QUERY_TABLE_DATA, "query_failed", &message, arguments);
        }
    };
    let mask_rules = match parse_mask_rules(allowlist.mask_rules, &allowed_columns, &actual_columns)
    {
        Ok(mask_rules) => mask_rules,
        Err(message) => {
            return tool_error_result(TOOL_QUERY_TABLE_DATA, "query_failed", &message, arguments);
        }
    };
    let requested_columns = match parse_columns_argument(arguments) {
        Ok(columns) => columns,
        Err(message) => return tool_argument_error(TOOL_QUERY_TABLE_DATA, &message, arguments),
    };
    let selected_columns =
        match select_columns(requested_columns, &allowed_columns, &actual_columns) {
            Ok(columns) => columns,
            Err(message) => return tool_argument_error(TOOL_QUERY_TABLE_DATA, &message, arguments),
        };
    let filters = match parse_filters_argument(arguments, &allowed_columns, &actual_columns) {
        Ok(filters) => filters,
        Err(message) => return tool_argument_error(TOOL_QUERY_TABLE_DATA, &message, arguments),
    };
    let order_by = match parse_order_by_argument(arguments, &allowed_columns, &actual_columns) {
        Ok(order_by) => order_by,
        Err(message) => return tool_argument_error(TOOL_QUERY_TABLE_DATA, &message, arguments),
    };
    let limit = match parse_limit(arguments, allowlist.max_limit) {
        Ok(limit) => limit,
        Err(message) => return tool_argument_error(TOOL_QUERY_TABLE_DATA, &message, arguments),
    };

    let explain_gate = match explain_table_query(
        &source_db,
        &table_path,
        &selected_columns,
        &filters,
        &order_by,
        limit,
        allowlist.max_estimated_rows,
    )
    .await
    {
        Ok(explain_gate) if explain_gate.passed => explain_gate,
        Ok(explain_gate) => {
            return explain_gate_error_result(
                TOOL_QUERY_TABLE_DATA,
                "query rejected by EXPLAIN gate",
                explain_gate,
                arguments,
            );
        }
        Err(error) => {
            return tool_error_result(
                TOOL_QUERY_TABLE_DATA,
                "query_failed",
                &format!("failed to explain database query: {error}"),
                arguments,
            );
        }
    };

    match execute_table_query(
        &source_db,
        &table_path,
        &selected_columns,
        &filters,
        &order_by,
        limit,
    )
    .await
    {
        Ok(rows) => {
            let (rows, masking) = mask_rows(rows, &selected_columns, &mask_rules);

            table_query_result(
                &source_ref,
                table_name,
                selected_columns,
                filters,
                order_by,
                limit,
                rows,
                explain_gate,
                masking,
                credential_version,
                arguments,
            )
        }
        Err(error) => tool_error_result(
            TOOL_QUERY_TABLE_DATA,
            "query_failed",
            &format!("database query failed: {error}"),
            arguments,
        ),
    }
}

const DEFAULT_KUBECTL_TIMEOUT_SECONDS: u64 = 10;
const DEFAULT_KUBECTL_OUTPUT_BYTES: usize = 65_536;
const MAX_KUBERNETES_SELECTOR_BYTES: usize = 256;

async fn list_kubernetes_resources(store: &FileStore, arguments: &Value) -> Value {
    let query = match parse_kubernetes_list_query(arguments) {
        Ok(query) => query,
        Err(message) => {
            return tool_argument_error(TOOL_LIST_KUBERNETES_RESOURCES, &message, arguments);
        }
    };
    let policy = match load_kubernetes_resource_policy(
        store,
        &query.source,
        &query.namespace,
        &query.resource,
        "list",
    )
    .await
    {
        Ok(Some(policy)) => policy,
        Ok(None) => {
            return tool_error_result(
                TOOL_LIST_KUBERNETES_RESOURCES,
                "not_allowed",
                "kubernetes resource list is not allowlisted",
                arguments,
            );
        }
        Err(message) => {
            return tool_error_result(
                TOOL_LIST_KUBERNETES_RESOURCES,
                "failed",
                &message,
                arguments,
            );
        }
    };
    let limit = match query.limit {
        Some(limit) if limit > policy.max_items => {
            return tool_error_result(
                TOOL_LIST_KUBERNETES_RESOURCES,
                "not_allowed",
                &format!("limit exceeds allowlist max_items {}", policy.max_items),
                arguments,
            );
        }
        Some(limit) => limit,
        None => policy.max_items.min(100),
    };

    let mut kubectl_args = vec![
        "get".to_string(),
        query.resource.clone(),
        "-n".to_string(),
        query.namespace.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    if let Some(selector) = &query.label_selector {
        kubectl_args.push("-l".to_string());
        kubectl_args.push(selector.clone());
    }
    if let Some(selector) = &query.field_selector {
        kubectl_args.push("--field-selector".to_string());
        kubectl_args.push(selector.clone());
    }

    match execute_kubectl_read_for_source(
        store,
        &query.source,
        &kubectl_args,
        DEFAULT_KUBECTL_TIMEOUT_SECONDS,
        policy.max_output_bytes,
    )
    .await
    {
        Ok((output, credential_version)) => kubernetes_list_result(
            query,
            policy,
            limit,
            kubectl_args,
            output,
            credential_version,
            arguments,
        ),
        Err(message) => tool_error_result(
            TOOL_LIST_KUBERNETES_RESOURCES,
            "failed",
            &message,
            arguments,
        ),
    }
}

async fn get_kubernetes_resource(store: &FileStore, arguments: &Value) -> Value {
    let query = match parse_kubernetes_get_query(arguments) {
        Ok(query) => query,
        Err(message) => {
            return tool_argument_error(TOOL_GET_KUBERNETES_RESOURCE, &message, arguments);
        }
    };
    let policy = match load_kubernetes_resource_policy(
        store,
        &query.source,
        &query.namespace,
        &query.resource,
        "get",
    )
    .await
    {
        Ok(Some(policy)) => policy,
        Ok(None) => {
            return tool_error_result(
                TOOL_GET_KUBERNETES_RESOURCE,
                "not_allowed",
                "kubernetes resource get is not allowlisted",
                arguments,
            );
        }
        Err(message) => {
            return tool_error_result(TOOL_GET_KUBERNETES_RESOURCE, "failed", &message, arguments);
        }
    };

    let kubectl_args = vec![
        "get".to_string(),
        query.resource.clone(),
        query.name.clone(),
        "-n".to_string(),
        query.namespace.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];

    match execute_kubectl_read_for_source(
        store,
        &query.source,
        &kubectl_args,
        DEFAULT_KUBECTL_TIMEOUT_SECONDS,
        policy.max_output_bytes,
    )
    .await
    {
        Ok((output, credential_version)) => kubernetes_get_result(
            query,
            policy,
            kubectl_args,
            output,
            credential_version,
            arguments,
        ),
        Err(message) => {
            tool_error_result(TOOL_GET_KUBERNETES_RESOURCE, "failed", &message, arguments)
        }
    }
}

async fn kubernetes_rollout_status(store: &FileStore, arguments: &Value) -> Value {
    let query = match parse_kubernetes_rollout_query(arguments) {
        Ok(query) => query,
        Err(message) => {
            return tool_argument_error(TOOL_KUBERNETES_ROLLOUT_STATUS, &message, arguments);
        }
    };
    let policy_action = match query.action.as_str() {
        "history" => "rollout_history",
        _ => "rollout_status",
    };
    let policy = match load_kubernetes_resource_policy(
        store,
        &query.source,
        &query.namespace,
        &query.resource,
        policy_action,
    )
    .await
    {
        Ok(Some(policy)) => policy,
        Ok(None) => {
            return tool_error_result(
                TOOL_KUBERNETES_ROLLOUT_STATUS,
                "not_allowed",
                "kubernetes rollout query is not allowlisted",
                arguments,
            );
        }
        Err(message) => {
            return tool_error_result(
                TOOL_KUBERNETES_ROLLOUT_STATUS,
                "failed",
                &message,
                arguments,
            );
        }
    };

    let mut kubectl_args = vec![
        "rollout".to_string(),
        query.action.clone(),
        format!("{}/{}", query.resource, query.name),
        "-n".to_string(),
        query.namespace.clone(),
    ];
    if query.action == "status" {
        kubectl_args.push("--watch=false".to_string());
    }
    if let Some(revision) = query.revision {
        kubectl_args.push(format!("--revision={revision}"));
    }

    match execute_kubectl_read_for_source(
        store,
        &query.source,
        &kubectl_args,
        DEFAULT_KUBECTL_TIMEOUT_SECONDS,
        policy.max_output_bytes,
    )
    .await
    {
        Ok((output, credential_version)) => kubernetes_rollout_result(
            query,
            policy,
            kubectl_args,
            output,
            credential_version,
            arguments,
        ),
        Err(message) => tool_error_result(
            TOOL_KUBERNETES_ROLLOUT_STATUS,
            "failed",
            &message,
            arguments,
        ),
    }
}

async fn query_pod_logs(store: &FileStore, arguments: &Value) -> Value {
    let query = match parse_pod_log_query(arguments) {
        Ok(query) => query,
        Err(message) => return tool_argument_error(TOOL_QUERY_POD_LOGS, &message, arguments),
    };
    let policy = match load_kubernetes_resource_policy(
        store,
        &query.source,
        &query.namespace,
        "pods",
        "logs",
    )
    .await
    {
        Ok(Some(policy)) => policy,
        Ok(None) => {
            return tool_error_result(
                TOOL_QUERY_POD_LOGS,
                "not_allowed",
                "kubernetes pod logs are not allowlisted",
                arguments,
            );
        }
        Err(message) => {
            return tool_error_result(TOOL_QUERY_POD_LOGS, "failed", &message, arguments);
        }
    };
    if query.tail_lines > policy.max_tail_lines {
        return tool_error_result(
            TOOL_QUERY_POD_LOGS,
            "not_allowed",
            &format!(
                "tail_lines exceeds allowlist max_tail_lines {}",
                policy.max_tail_lines
            ),
            arguments,
        );
    }
    if query.max_output_bytes > policy.max_output_bytes {
        return tool_error_result(
            TOOL_QUERY_POD_LOGS,
            "not_allowed",
            &format!(
                "max_output_bytes exceeds allowlist max_output_bytes {}",
                policy.max_output_bytes
            ),
            arguments,
        );
    }

    match execute_kubectl_read_for_source(
        store,
        &query.source,
        &query.kubectl_args,
        query.timeout_seconds,
        query.max_output_bytes,
    )
    .await
    {
        Ok((output, credential_version)) => {
            pod_log_query_result(query, policy, output, credential_version, arguments)
        }
        Err(message) => tool_error_result(TOOL_QUERY_POD_LOGS, "failed", &message, arguments),
    }
}

const DEFAULT_APP_LOG_LIMIT: usize = 50;
const MAX_APP_LOG_LIMIT: usize = 200;
const MAX_APP_LOG_CANDIDATES: usize = 1000;
const MAX_APP_LOG_ENTRY_BYTES: usize = 16_384;
const MAX_APP_LOG_TOTAL_BYTES: usize = 65_536;
const MAX_APP_LOG_MESSAGE_CHARS: usize = 512;
const MAX_APP_LOG_FIELD_VALUE_CHARS: usize = 256;

async fn query_app_logs(store: &FileStore, redis: &RedisClient, arguments: &Value) -> Value {
    let source_ref = match source_ref_from_tool_arguments(arguments) {
        Ok(source_ref) => source_ref,
        Err(message) => return tool_argument_error(TOOL_QUERY_APP_LOGS, &message, arguments),
    };
    let query = match parse_app_log_query(arguments) {
        Ok(query) => query,
        Err(message) => return tool_argument_error(TOOL_QUERY_APP_LOGS, &message, arguments),
    };

    let (source_redis, credential_version) =
        match redis_client_for_source(store, &source_ref, "logs_redis").await {
            Ok((Some(source_redis), credential_version)) => (source_redis, credential_version),
            Ok((None, credential_version)) => (redis.clone(), credential_version),
            Err(message) => {
                return tool_error_result(TOOL_QUERY_APP_LOGS, "query_failed", &message, arguments);
            }
        };

    match execute_app_log_query(&source_redis, &query).await {
        Ok(read) => app_log_query_result(source_ref, credential_version, query, read, arguments),
        Err(AppLogQueryError::IndexMissing) => tool_error_result(
            TOOL_QUERY_APP_LOGS,
            "not_allowed",
            "application log index is not available",
            arguments,
        ),
        Err(AppLogQueryError::QueryFailed(message)) => tool_error_result(
            TOOL_QUERY_APP_LOGS,
            "query_failed",
            &format!("application log query failed: {message}"),
            arguments,
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppLogQuery {
    app_name: String,
    environment: Option<String>,
    trace_id: Option<String>,
    keyword: Option<String>,
    since: Option<String>,
    min_score_millis: Option<i64>,
    limit: usize,
    index_key: String,
}

#[derive(Debug, Clone, PartialEq)]
struct AppLogRead {
    entries: Vec<Value>,
    scanned_count: usize,
    returned_count: usize,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppLogQueryError {
    IndexMissing,
    QueryFailed(String),
}

fn parse_app_log_query(arguments: &Value) -> Result<AppLogQuery, String> {
    let app_name = required_string_argument(arguments, "app_name")?.to_string();
    validate_app_log_name(&app_name, "app_name", 128)?;

    let environment = optional_string_argument(arguments, "environment")?
        .map(|environment| {
            validate_app_log_name(environment, "environment", 64)?;
            Ok::<_, String>(environment.to_string())
        })
        .transpose()?;
    let trace_id = optional_string_argument(arguments, "trace_id")?
        .map(|trace_id| {
            validate_app_log_text(trace_id, "trace_id", 128)?;
            Ok::<_, String>(trace_id.to_string())
        })
        .transpose()?;
    let keyword = optional_string_argument(arguments, "keyword")?
        .map(|keyword| {
            validate_app_log_text(keyword, "keyword", 128)?;
            Ok::<_, String>(keyword.to_string())
        })
        .transpose()?;
    let since = optional_string_argument(arguments, "since")?.map(str::to_string);
    let min_score_millis = since
        .as_deref()
        .map(parse_app_log_since_millis)
        .transpose()?;
    let limit = optional_usize_argument(arguments, "limit", 1, MAX_APP_LOG_LIMIT)?
        .unwrap_or(DEFAULT_APP_LOG_LIMIT);
    let index_key = app_log_index_key(&app_name, environment.as_deref());

    Ok(AppLogQuery {
        app_name,
        environment,
        trace_id,
        keyword,
        since,
        min_score_millis,
        limit,
        index_key,
    })
}

fn validate_app_log_name(value: &str, name: &str, max_bytes: usize) -> Result<(), String> {
    if value.len() > max_bytes {
        return Err(format!("{name} must be {max_bytes} bytes or fewer"));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(format!(
            "{name} may contain only ASCII letters, numbers, '.', '-', and '_'"
        ));
    }

    Ok(())
}

fn validate_app_log_text(value: &str, name: &str, max_chars: usize) -> Result<(), String> {
    if value.chars().count() > max_chars {
        return Err(format!("{name} must be {max_chars} characters or fewer"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{name} must not contain control characters"));
    }

    Ok(())
}

fn parse_app_log_since_millis(value: &str) -> Result<i64, String> {
    let duration_millis = parse_app_log_duration_millis(value)?;
    let now_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_millis();
    let min_score = now_millis.saturating_sub(u128::from(duration_millis));

    i64::try_from(min_score).map_err(|_| "current time is too large".to_string())
}

fn parse_app_log_duration_millis(value: &str) -> Result<u64, String> {
    if value.is_empty() || value.len() > 32 {
        return Err("since must be a duration such as 15m or 1h".to_string());
    }

    let split_index = value
        .find(|character: char| !character.is_ascii_digit())
        .ok_or_else(|| "since must be a duration such as 15m or 1h".to_string())?;
    let (amount, unit) = value.split_at(split_index);
    if amount.is_empty()
        || unit.is_empty()
        || unit.chars().any(|character| character.is_ascii_digit())
    {
        return Err("since must be a duration such as 15m or 1h".to_string());
    }

    let amount = amount
        .parse::<u64>()
        .map_err(|_| "since must be a duration such as 15m or 1h".to_string())?;
    if amount == 0 {
        return Err("since must be greater than zero".to_string());
    }
    let multiplier = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err("since must be a duration such as 15m or 1h".to_string()),
    };

    amount
        .checked_mul(multiplier)
        .ok_or_else(|| "since duration is too large".to_string())
}

fn app_log_index_key(app_name: &str, environment: Option<&str>) -> String {
    match environment {
        Some(environment) => format!("app_logs:index:app_env:{app_name}:{environment}"),
        None => format!("app_logs:index:app:{app_name}"),
    }
}

async fn execute_app_log_query(
    redis: &RedisClient,
    query: &AppLogQuery,
) -> Result<AppLogRead, AppLogQueryError> {
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|error| AppLogQueryError::QueryFailed(error.to_string()))?;

    let exists = redis::cmd("EXISTS")
        .arg(&query.index_key)
        .query_async::<bool>(&mut connection)
        .await
        .map_err(|error| AppLogQueryError::QueryFailed(error.to_string()))?;
    if !exists {
        return Err(AppLogQueryError::IndexMissing);
    }

    let min_score = query
        .min_score_millis
        .map_or_else(|| "-inf".to_string(), |score| score.to_string());
    let candidate_limit = app_log_candidate_limit(query.limit);
    let ids = redis::cmd("ZREVRANGEBYSCORE")
        .arg(&query.index_key)
        .arg("+inf")
        .arg(min_score)
        .arg("LIMIT")
        .arg(0)
        .arg(candidate_limit)
        .query_async::<Vec<String>>(&mut connection)
        .await
        .map_err(|error| AppLogQueryError::QueryFailed(error.to_string()))?;

    let mut entries = Vec::new();
    let mut total_output_bytes = 0_usize;
    let mut scanned_count = 0_usize;
    let mut truncated = false;

    for id in &ids {
        scanned_count += 1;
        let entry_key = format!("app_logs:entry:{id}");
        let Some(bytes) = redis::cmd("GET")
            .arg(&entry_key)
            .query_async::<Option<Vec<u8>>>(&mut connection)
            .await
            .map_err(|error| AppLogQueryError::QueryFailed(error.to_string()))?
        else {
            continue;
        };
        if bytes.len() > MAX_APP_LOG_ENTRY_BYTES {
            return Err(AppLogQueryError::QueryFailed(format!(
                "log entry {id} exceeds max entry size {MAX_APP_LOG_ENTRY_BYTES} bytes"
            )));
        }

        let raw = String::from_utf8(bytes).map_err(|error| {
            AppLogQueryError::QueryFailed(format!("log entry {id} is not valid UTF-8: {error}"))
        })?;
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            AppLogQueryError::QueryFailed(format!("log entry {id} is not valid JSON: {error}"))
        })?;
        if !app_log_entry_matches(query, &value) {
            continue;
        }

        let summary = summarize_app_log_entry(&value).map_err(AppLogQueryError::QueryFailed)?;
        let summary_bytes = serde_json::to_vec(&summary)
            .map_err(|error| AppLogQueryError::QueryFailed(error.to_string()))?
            .len();
        if total_output_bytes.saturating_add(summary_bytes) > MAX_APP_LOG_TOTAL_BYTES {
            truncated = true;
            break;
        }

        total_output_bytes += summary_bytes;
        entries.push(summary);
        if entries.len() >= query.limit {
            truncated = scanned_count < ids.len() || ids.len() >= candidate_limit;
            break;
        }
    }

    if ids.len() >= candidate_limit && entries.len() < query.limit {
        truncated = true;
    }

    Ok(AppLogRead {
        returned_count: entries.len(),
        entries,
        scanned_count,
        truncated,
    })
}

fn app_log_candidate_limit(limit: usize) -> usize {
    limit
        .saturating_mul(10)
        .clamp(limit, MAX_APP_LOG_CANDIDATES)
}

fn app_log_entry_matches(query: &AppLogQuery, value: &Value) -> bool {
    if value.get("app_name").and_then(Value::as_str) != Some(query.app_name.as_str()) {
        return false;
    }
    if let Some(environment) = &query.environment
        && value.get("environment").and_then(Value::as_str) != Some(environment.as_str())
    {
        return false;
    }
    if let Some(trace_id) = &query.trace_id
        && value.get("trace_id").and_then(Value::as_str) != Some(trace_id.as_str())
    {
        return false;
    }
    if let Some(keyword) = &query.keyword {
        return app_log_entry_contains_keyword(value, keyword);
    }

    true
}

fn app_log_entry_contains_keyword(value: &Value, keyword: &str) -> bool {
    let keyword = keyword.to_ascii_lowercase();
    for field in ["level", "trace_id", "message"] {
        if value
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_ascii_lowercase().contains(&keyword))
        {
            return true;
        }
    }
    value
        .get("fields")
        .and_then(|fields| serde_json::to_string(fields).ok())
        .is_some_and(|fields| fields.to_ascii_lowercase().contains(&keyword))
}

fn summarize_app_log_entry(value: &Value) -> Result<Value, String> {
    let id = required_log_entry_string(value, "id")?;
    let timestamp = required_log_entry_string(value, "timestamp")?;
    let level = required_log_entry_string(value, "level")?;
    let message = required_log_entry_string(value, "message")?;
    let trace_id = value.get("trace_id").and_then(Value::as_str);
    let app_name = value.get("app_name").and_then(Value::as_str);
    let environment = value.get("environment").and_then(Value::as_str);
    let fields = value
        .get("fields")
        .map(summarize_app_log_fields)
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

    Ok(json!({
        "id": truncate_string(id, 128),
        "timestamp": truncate_string(timestamp, 64),
        "level": truncate_string(level, 32),
        "appName": app_name.map(|value| truncate_string(value, 128)),
        "environment": environment.map(|value| truncate_string(value, 64)),
        "traceId": trace_id.map(|value| truncate_string(value, 128)),
        "message": truncate_string(message, MAX_APP_LOG_MESSAGE_CHARS),
        "fields": fields
    }))
}

fn required_log_entry_string<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("log entry missing string field {name}"))
}

fn summarize_app_log_fields(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return Value::Object(serde_json::Map::new());
    };

    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort();
    let mut summary = serde_json::Map::new();
    for key in keys.into_iter().take(32) {
        let Some(value) = object.get(key) else {
            continue;
        };
        summary.insert(
            key.chars().take(128).collect::<String>(),
            truncate_json_string(value, MAX_APP_LOG_FIELD_VALUE_CHARS),
        );
    }

    Value::Object(summary)
}

fn app_log_query_result(
    source: control_plane::SourceRef,
    credential_version: Option<i64>,
    query: AppLogQuery,
    read: AppLogRead,
    arguments: &Value,
) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": format!(
                    "{TOOL_QUERY_APP_LOGS}: returned {} log event(s) for {}",
                    read.returned_count,
                    query.app_name
                )
            }
        ],
        "structuredContent": {
            "status": "succeeded",
            "action": TOOL_QUERY_APP_LOGS,
            "sourceName": source.source_name,
            "credentialVersion": credential_version,
            "appName": query.app_name,
            "environment": query.environment,
            "traceId": query.trace_id,
            "keywordPresent": query.keyword.is_some(),
            "since": query.since,
            "indexKey": query.index_key,
            "limit": query.limit,
            "scannedCount": read.scanned_count,
            "returnedCount": read.returned_count,
            "truncated": read.truncated,
            "logs": read.entries,
            "receivedArguments": arguments
        },
        "isError": false
    })
}

async fn run_kubectl_read(store: &FileStore, arguments: &Value) -> Value {
    if !raw_kubectl_enabled() {
        return tool_error_result(
            TOOL_RUN_KUBECTL_READ,
            "not_allowed",
            "raw kubectl is disabled; set KUBERNETES_ENABLE_RAW_KUBECTL=true to expose this diagnostic escape hatch",
            arguments,
        );
    }

    let source = match source_ref_from_tool_arguments(arguments) {
        Ok(source) => source,
        Err(message) => return tool_argument_error(TOOL_RUN_KUBECTL_READ, &message, arguments),
    };
    let args = match parse_kubectl_args(arguments) {
        Ok(args) => args,
        Err(message) => return tool_argument_error(TOOL_RUN_KUBECTL_READ, &message, arguments),
    };
    if let Err(message) = validate_kubectl_read_args(&args) {
        return tool_argument_error(TOOL_RUN_KUBECTL_READ, &message, arguments);
    }
    let timeout_seconds = match parse_u64_argument(arguments, "timeout_seconds", 10, 1, 60) {
        Ok(timeout_seconds) => timeout_seconds,
        Err(message) => return tool_argument_error(TOOL_RUN_KUBECTL_READ, &message, arguments),
    };
    let requested_max_output_bytes = match parse_usize_argument(
        arguments,
        "max_output_bytes",
        DEFAULT_KUBECTL_OUTPUT_BYTES,
        1_024,
        1_048_576,
    ) {
        Ok(max_output_bytes) => max_output_bytes,
        Err(message) => return tool_argument_error(TOOL_RUN_KUBECTL_READ, &message, arguments),
    };
    let (max_output_bytes, policy) = match raw_kubectl_policy_subject(&args) {
        Ok(RawKubectlPolicySubject::Cluster) => (
            requested_max_output_bytes.min(DEFAULT_KUBECTL_OUTPUT_BYTES),
            None,
        ),
        Ok(RawKubectlPolicySubject::Resource {
            namespace,
            resource,
            action,
            ..
        }) => {
            let policy = match load_kubernetes_resource_policy(
                store, &source, &namespace, &resource, &action,
            )
            .await
            {
                Ok(Some(policy)) => policy,
                Ok(None) => {
                    return tool_error_result(
                        TOOL_RUN_KUBECTL_READ,
                        "not_allowed",
                        "raw kubectl target is not allowlisted",
                        arguments,
                    );
                }
                Err(message) => {
                    return tool_error_result(TOOL_RUN_KUBECTL_READ, "failed", &message, arguments);
                }
            };
            if requested_max_output_bytes > policy.max_output_bytes {
                return tool_error_result(
                    TOOL_RUN_KUBECTL_READ,
                    "not_allowed",
                    &format!(
                        "max_output_bytes exceeds allowlist max_output_bytes {}",
                        policy.max_output_bytes
                    ),
                    arguments,
                );
            }
            (requested_max_output_bytes, Some(policy))
        }
        Err(message) => return tool_argument_error(TOOL_RUN_KUBECTL_READ, &message, arguments),
    };

    match execute_kubectl_read_for_source(store, &source, &args, timeout_seconds, max_output_bytes)
        .await
    {
        Ok((output, credential_version)) => kubectl_run_result(
            args,
            timeout_seconds,
            max_output_bytes,
            policy,
            output,
            credential_version,
            arguments,
        ),
        Err(message) => tool_error_result(TOOL_RUN_KUBECTL_READ, "failed", &message, arguments),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KubernetesListQuery {
    source: control_plane::SourceRef,
    namespace: String,
    resource: String,
    label_selector: Option<String>,
    field_selector: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KubernetesGetQuery {
    source: control_plane::SourceRef,
    namespace: String,
    resource: String,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KubernetesRolloutQuery {
    source: control_plane::SourceRef,
    namespace: String,
    resource: String,
    name: String,
    action: String,
    revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PodLogQuery {
    source: control_plane::SourceRef,
    namespace: String,
    pod_name: String,
    container: Option<String>,
    since: Option<String>,
    tail_lines: u64,
    previous: bool,
    timestamps: bool,
    timeout_seconds: u64,
    max_output_bytes: usize,
    kubectl_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KubernetesResourcePolicy {
    source_name: String,
    namespace: String,
    resource: String,
    actions: Vec<String>,
    max_items: usize,
    max_output_bytes: usize,
    max_tail_lines: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RawKubectlPolicySubject {
    Cluster,
    Resource {
        namespace: String,
        resource: String,
        name: Option<String>,
        action: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KubectlRunOutput {
    exit_code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

fn parse_kubernetes_list_query(arguments: &Value) -> Result<KubernetesListQuery, String> {
    let source = source_ref_from_tool_arguments(arguments)?;
    let namespace = required_string_argument(arguments, "namespace")?.to_string();
    let resource = normalize_kubernetes_resource(required_string_argument(arguments, "resource")?)?;
    validate_kubernetes_dns_label(&namespace, "namespace")?;
    let label_selector = optional_kubernetes_selector(arguments, "label_selector")?;
    let field_selector = optional_kubernetes_selector(arguments, "field_selector")?;
    let limit = optional_usize_argument(arguments, "limit", 1, 1000)?;

    Ok(KubernetesListQuery {
        source,
        namespace,
        resource,
        label_selector,
        field_selector,
        limit,
    })
}

fn parse_kubernetes_get_query(arguments: &Value) -> Result<KubernetesGetQuery, String> {
    let source = source_ref_from_tool_arguments(arguments)?;
    let namespace = required_string_argument(arguments, "namespace")?.to_string();
    let resource = normalize_kubernetes_resource(required_string_argument(arguments, "resource")?)?;
    let name = required_string_argument(arguments, "name")?.to_string();
    validate_kubernetes_dns_label(&namespace, "namespace")?;
    validate_kubernetes_dns_subdomain(&name, "name")?;

    Ok(KubernetesGetQuery {
        source,
        namespace,
        resource,
        name,
    })
}

fn parse_kubernetes_rollout_query(arguments: &Value) -> Result<KubernetesRolloutQuery, String> {
    let source = source_ref_from_tool_arguments(arguments)?;
    let namespace = required_string_argument(arguments, "namespace")?.to_string();
    let resource = normalize_kubernetes_resource(required_string_argument(arguments, "resource")?)?;
    let name = required_string_argument(arguments, "name")?.to_string();
    let action = optional_string_argument(arguments, "action")?
        .unwrap_or("status")
        .to_string();
    let revision = optional_u64_argument(arguments, "revision", 1, 1_000_000)?;

    validate_kubernetes_dns_label(&namespace, "namespace")?;
    validate_kubernetes_dns_subdomain(&name, "name")?;
    if !matches!(
        resource.as_str(),
        "deployments" | "statefulsets" | "daemonsets"
    ) {
        return Err("resource must be deployments, statefulsets, or daemonsets".to_string());
    }
    if !matches!(action.as_str(), "status" | "history") {
        return Err("action must be status or history".to_string());
    }
    if revision.is_some() && action != "history" {
        return Err("revision is only supported for rollout history".to_string());
    }

    Ok(KubernetesRolloutQuery {
        source,
        namespace,
        resource,
        name,
        action,
        revision,
    })
}

fn parse_pod_log_query(arguments: &Value) -> Result<PodLogQuery, String> {
    let source = source_ref_from_tool_arguments(arguments)?;
    let namespace = required_string_argument(arguments, "namespace")?.to_string();
    let pod_name = required_string_argument(arguments, "pod_name")?.to_string();
    let container = optional_string_argument(arguments, "container")?.map(str::to_string);
    let since = optional_string_argument(arguments, "since")?.map(str::to_string);
    let tail_lines = parse_u64_argument(arguments, "tail_lines", 200, 1, 5000)?;
    let previous = parse_bool_argument(arguments, "previous", false)?;
    let timestamps = parse_bool_argument(arguments, "timestamps", false)?;
    let timeout_seconds = parse_u64_argument(arguments, "timeout_seconds", 10, 1, 60)?;
    let max_output_bytes = parse_usize_argument(
        arguments,
        "max_output_bytes",
        DEFAULT_KUBECTL_OUTPUT_BYTES,
        1_024,
        1_048_576,
    )?;

    validate_kubernetes_dns_label(&namespace, "namespace")?;
    validate_kubernetes_dns_subdomain(&pod_name, "pod_name")?;
    if let Some(container) = &container {
        validate_kubernetes_dns_label(container, "container")?;
    }
    if let Some(since) = &since {
        validate_kubernetes_duration(since, "since")?;
    }

    let mut kubectl_args = vec![
        "logs".to_string(),
        pod_name.clone(),
        "-n".to_string(),
        namespace.clone(),
        "--tail".to_string(),
        tail_lines.to_string(),
    ];
    if let Some(container) = &container {
        kubectl_args.push("-c".to_string());
        kubectl_args.push(container.clone());
    }
    if let Some(since) = &since {
        kubectl_args.push("--since".to_string());
        kubectl_args.push(since.clone());
    }
    if previous {
        kubectl_args.push("--previous".to_string());
    }
    if timestamps {
        kubectl_args.push("--timestamps".to_string());
    }

    Ok(PodLogQuery {
        source,
        namespace,
        pod_name,
        container,
        since,
        tail_lines,
        previous,
        timestamps,
        timeout_seconds,
        max_output_bytes,
        kubectl_args,
    })
}

async fn load_kubernetes_resource_policy(
    store: &FileStore,
    source: &control_plane::SourceRef,
    namespace: &str,
    resource: &str,
    action: &str,
) -> Result<Option<KubernetesResourcePolicy>, String> {
    let Some(record) = store
        .kubernetes_policy(&source.source_name, namespace, resource)
        .await
    else {
        return Ok(None);
    };
    let actions = parse_kubernetes_policy_actions(Value::Array(
        record
            .actions
            .iter()
            .map(|action| Value::String(action.clone()))
            .collect(),
    ))?;
    if !actions
        .iter()
        .any(|allowed_action| allowed_action == action)
    {
        return Ok(None);
    }

    Ok(Some(KubernetesResourcePolicy {
        source_name: record.source_name,
        namespace: record.namespace,
        resource: record.resource,
        actions,
        max_items: record.max_items.clamp(1, 1000),
        max_output_bytes: record.max_output_bytes.clamp(1_024, 1_048_576),
        max_tail_lines: record.max_tail_lines.clamp(1, 5000),
    }))
}

fn parse_kubernetes_policy_actions(actions: Value) -> Result<Vec<String>, String> {
    let array = actions
        .as_array()
        .ok_or_else(|| "kubernetes allowlist actions must be a JSON array".to_string())?;
    let mut parsed = Vec::with_capacity(array.len());
    for action in array {
        let action = action
            .as_str()
            .ok_or_else(|| "kubernetes allowlist actions must be strings".to_string())?;
        if action.is_empty() || action.len() > 64 || action.chars().any(char::is_control) {
            return Err("kubernetes allowlist action is invalid".to_string());
        }
        if !parsed.iter().any(|existing| existing == action) {
            parsed.push(action.to_string());
        }
    }

    Ok(parsed)
}

fn required_string_argument<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    match arguments.get(name) {
        Some(Value::String(value)) if !value.is_empty() => Ok(value),
        Some(Value::String(_)) | None => Err(format!("missing required argument: {name}")),
        Some(_) => Err(format!("{name} must be a string")),
    }
}

fn optional_string_argument<'a>(
    arguments: &'a Value,
    name: &str,
) -> Result<Option<&'a str>, String> {
    match arguments.get(name) {
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(Value::String(_)) => Err(format!("{name} must not be empty")),
        Some(_) => Err(format!("{name} must be a string")),
        None => Ok(None),
    }
}

fn optional_kubernetes_selector(arguments: &Value, name: &str) -> Result<Option<String>, String> {
    let Some(selector) = optional_string_argument(arguments, name)? else {
        return Ok(None);
    };
    if selector.len() > MAX_KUBERNETES_SELECTOR_BYTES {
        return Err(format!(
            "{name} must be {MAX_KUBERNETES_SELECTOR_BYTES} bytes or fewer"
        ));
    }
    let valid = selector.chars().all(|character| {
        character.is_ascii_alphanumeric()
            || matches!(
                character,
                '-' | '_' | '.' | '/' | '=' | '!' | ',' | '(' | ')'
            )
    });
    if !valid {
        return Err(format!("{name} contains unsupported characters"));
    }

    Ok(Some(selector.to_string()))
}

fn parse_bool_argument(arguments: &Value, name: &str, default: bool) -> Result<bool, String> {
    match arguments.get(name) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{name} must be a boolean")),
        None => Ok(default),
    }
}

fn parse_u64_argument(
    arguments: &Value,
    name: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, String> {
    Ok(optional_u64_argument(arguments, name, min, max)?.unwrap_or(default))
}

fn optional_u64_argument(
    arguments: &Value,
    name: &str,
    min: u64,
    max: u64,
) -> Result<Option<u64>, String> {
    let Some(value) = arguments.get(name) else {
        return Ok(None);
    };
    let value = match value {
        Value::Number(number) => number
            .as_u64()
            .ok_or_else(|| format!("{name} must be a positive integer"))?,
        _ => return Err(format!("{name} must be a positive integer")),
    };
    if value < min || value > max {
        return Err(format!("{name} must be between {min} and {max}"));
    }

    Ok(Some(value))
}

fn parse_usize_argument(
    arguments: &Value,
    name: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, String> {
    Ok(optional_usize_argument(arguments, name, min, max)?.unwrap_or(default))
}

fn optional_usize_argument(
    arguments: &Value,
    name: &str,
    min: usize,
    max: usize,
) -> Result<Option<usize>, String> {
    let Some(value) = arguments.get(name) else {
        return Ok(None);
    };
    let value = match value {
        Value::Number(number) => number
            .as_u64()
            .ok_or_else(|| format!("{name} must be a positive integer"))?,
        _ => return Err(format!("{name} must be a positive integer")),
    };
    if value < min as u64 || value > max as u64 {
        return Err(format!("{name} must be between {min} and {max}"));
    }

    usize::try_from(value)
        .map(Some)
        .map_err(|_| format!("{name} is too large"))
}

fn validate_kubernetes_dns_label(value: &str, name: &str) -> Result<(), String> {
    if value.len() > 63 {
        return Err(format!("{name} must be 63 bytes or fewer"));
    }
    validate_kubernetes_name_chars(value, name, false)
}

fn validate_kubernetes_dns_subdomain(value: &str, name: &str) -> Result<(), String> {
    if value.len() > 253 {
        return Err(format!("{name} must be 253 bytes or fewer"));
    }
    validate_kubernetes_name_chars(value, name, true)
}

fn validate_kubernetes_name_chars(value: &str, name: &str, allow_dot: bool) -> Result<(), String> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(format!("{name} must not be empty"));
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(format!(
            "{name} must start with a lowercase alphanumeric character"
        ));
    }
    let mut previous = first;
    for character in chars {
        let allowed = character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || (allow_dot && character == '.');
        if !allowed {
            return Err(format!(
                "{name} must contain only lowercase alphanumeric characters, '-'{}",
                if allow_dot { ", or '.'" } else { "" }
            ));
        }
        previous = character;
    }
    if !previous.is_ascii_lowercase() && !previous.is_ascii_digit() {
        return Err(format!(
            "{name} must end with a lowercase alphanumeric character"
        ));
    }

    Ok(())
}

fn validate_kubernetes_duration(value: &str, name: &str) -> Result<(), String> {
    if value.len() > 32 {
        return Err(format!("{name} must be 32 bytes or fewer"));
    }
    let regex = Regex::new(r"^[0-9]+(ns|us|ms|s|m|h)([0-9]+(ns|us|ms|s|m|h))*$")
        .map_err(|error| error.to_string())?;
    if !regex.is_match(value) {
        return Err(format!("{name} must be a duration such as 15m or 1h"));
    }

    Ok(())
}

fn normalize_kubernetes_resource(resource: &str) -> Result<String, String> {
    if resource.len() > 128 {
        return Err("resource must be 128 bytes or fewer".to_string());
    }
    validate_kubernetes_dns_subdomain(resource, "resource")?;
    let normalized = match resource {
        "po" | "pod" | "pods" => "pods",
        "deploy" | "deploys" | "deployment" | "deployments" => "deployments",
        "sts" | "statefulset" | "statefulsets" => "statefulsets",
        "ds" | "daemonset" | "daemonsets" => "daemonsets",
        "svc" | "service" | "services" => "services",
        "ing" | "ingress" | "ingresses" => "ingresses",
        "job" | "jobs" => "jobs",
        "pvc" | "pvcs" | "persistentvolumeclaim" | "persistentvolumeclaims" => {
            "persistentvolumeclaims"
        }
        "ev" | "event" | "events" => "events",
        other => other,
    };

    Ok(normalized.to_string())
}

pub(crate) fn raw_kubectl_enabled() -> bool {
    raw_kubectl_enabled_value(
        std::env::var("KUBERNETES_ENABLE_RAW_KUBECTL")
            .ok()
            .as_deref(),
    )
}

fn raw_kubectl_enabled_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        )
    })
}

fn parse_kubectl_args(arguments: &Value) -> Result<Vec<String>, String> {
    let args = arguments
        .get("args")
        .ok_or_else(|| "missing required argument: args".to_string())?
        .as_array()
        .ok_or_else(|| "args must be an array of strings".to_string())?;

    if args.is_empty() {
        return Err("args must include at least one kubectl argument".to_string());
    }
    if args.len() > 64 {
        return Err("args must include 64 or fewer entries".to_string());
    }

    let mut parsed = Vec::with_capacity(args.len());
    let mut total_bytes = 0_usize;
    for arg in args {
        let arg = arg
            .as_str()
            .ok_or_else(|| "args must be an array of strings".to_string())?;
        if arg.is_empty() {
            return Err("kubectl arguments must not be empty".to_string());
        }
        if arg.len() > 512 {
            return Err("each kubectl argument must be 512 bytes or fewer".to_string());
        }
        if arg.chars().any(char::is_control) {
            return Err("kubectl arguments must not contain control characters".to_string());
        }
        total_bytes = total_bytes.saturating_add(arg.len());
        parsed.push(arg.to_string());
    }
    if total_bytes > 8_192 {
        return Err("kubectl arguments are too large".to_string());
    }

    Ok(parsed)
}

fn validate_kubectl_read_args(args: &[String]) -> Result<(), String> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| "args must include at least one kubectl argument".to_string())?;
    if command.starts_with('-') {
        return Err("first kubectl argument must be a diagnostic command".to_string());
    }

    match command {
        "get" | "describe" | "api-resources" | "api-versions" | "version" => {}
        "config" => validate_kubectl_subcommand(args, "config", &["current-context"])?,
        "rollout" => validate_kubectl_subcommand(args, "rollout", &["history", "status"])?,
        _ => {
            return Err(format!(
                "kubectl command is not allowed for raw diagnostic execution: {command}"
            ));
        }
    }

    validate_raw_kubectl_flags(args)
}

fn validate_kubectl_subcommand(
    args: &[String],
    command: &str,
    allowed_subcommands: &[&str],
) -> Result<(), String> {
    let subcommand = args
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| format!("kubectl {command} requires an allowed diagnostic subcommand"))?;
    if subcommand.starts_with('-') || !allowed_subcommands.contains(&subcommand) {
        return Err(format!(
            "kubectl {command} subcommand is not allowed: {subcommand}"
        ));
    }

    Ok(())
}

fn validate_raw_kubectl_flags(args: &[String]) -> Result<(), String> {
    let mut index = 0_usize;
    while index < args.len() {
        let arg = &args[index];
        validate_kubectl_safe_arg(arg)?;
        if matches!(arg.as_str(), "-o" | "--output") {
            let output = args
                .get(index + 1)
                .ok_or_else(|| format!("kubectl flag requires a value: {arg}"))?;
            validate_raw_kubectl_output_format(output)?;
            index += 2;
            continue;
        }
        if let Some(output) = arg
            .strip_prefix("-o=")
            .or_else(|| arg.strip_prefix("--output="))
        {
            validate_raw_kubectl_output_format(output)?;
        }
        if arg.starts_with("-o") && arg.len() > 2 && !arg.starts_with("-o=") {
            validate_raw_kubectl_output_format(&arg[2..])?;
        }
        index += 1;
    }

    Ok(())
}

fn validate_raw_kubectl_output_format(output: &str) -> Result<(), String> {
    let normalized = output.trim();
    if normalized == "wide" {
        return Ok(());
    }

    Err(format!("kubectl output format is not allowed: {output}"))
}

fn validate_kubectl_safe_arg(arg: &str) -> Result<(), String> {
    if arg == "--" {
        return Err("kubectl argument separator is not allowed".to_string());
    }
    if matches!(
        arg,
        "-A" | "--all-namespaces" | "-w" | "--watch" | "--watch-only" | "-f" | "-R" | "--recursive"
    ) {
        return Err(format!("kubectl flag is not allowed: {arg}"));
    }
    if arg.starts_with("-A=")
        || arg.starts_with("--all-namespaces=")
        || arg.starts_with("--watch=")
        || arg.starts_with("--watch-only=")
        || arg.starts_with("--follow=")
    {
        return Err(format!("kubectl flag is not allowed: {arg}"));
    }

    const DISALLOWED_FLAG_PREFIXES: &[&str] = &[
        "--as",
        "--as-group",
        "--as-uid",
        "--cache-dir",
        "--certificate-authority",
        "--client-certificate",
        "--client-key",
        "--cluster",
        "--context",
        "--field-manager",
        "--filename",
        "--force",
        "--grace-period",
        "--insecure-skip-tls-verify",
        "--kubeconfig",
        "--kuberc",
        "--password",
        "--proxy-url",
        "--raw",
        "--request-timeout",
        "--server",
        "--sort-by",
        "--template",
        "--token",
        "--user",
        "--username",
        "--watch",
        "--watch-only",
    ];
    for prefix in DISALLOWED_FLAG_PREFIXES {
        if arg == *prefix || arg.starts_with(&format!("{prefix}=")) {
            return Err(format!("kubectl flag is not allowed: {arg}"));
        }
    }

    Ok(())
}

fn raw_kubectl_policy_subject(args: &[String]) -> Result<RawKubectlPolicySubject, String> {
    match args.first().map(String::as_str) {
        Some("api-resources" | "api-versions" | "version") => Ok(RawKubectlPolicySubject::Cluster),
        Some("config") => Ok(RawKubectlPolicySubject::Cluster),
        Some("get") => {
            let namespace = raw_kubectl_namespace(args)?;
            let positionals = raw_kubectl_positionals(args, 1);
            let (resource, name) = raw_kubectl_resource_and_name(&positionals)?;
            let action = if name.is_some() { "get" } else { "list" };
            Ok(RawKubectlPolicySubject::Resource {
                namespace,
                resource,
                name,
                action: action.to_string(),
            })
        }
        Some("describe") => {
            let namespace = raw_kubectl_namespace(args)?;
            let positionals = raw_kubectl_positionals(args, 1);
            let (resource, name) = raw_kubectl_resource_and_name(&positionals)?;
            let Some(name) = name else {
                return Err("kubectl describe requires a resource name".to_string());
            };
            Ok(RawKubectlPolicySubject::Resource {
                namespace,
                resource,
                name: Some(name),
                action: "describe".to_string(),
            })
        }
        Some("rollout") => {
            let subcommand = args
                .get(1)
                .map(String::as_str)
                .ok_or_else(|| "kubectl rollout requires status or history".to_string())?;
            let namespace = raw_kubectl_namespace(args)?;
            let positionals = raw_kubectl_positionals(args, 2);
            let (resource, name) = raw_kubectl_resource_and_name(&positionals)?;
            let Some(name) = name else {
                return Err("kubectl rollout requires a resource name".to_string());
            };
            if !matches!(
                resource.as_str(),
                "deployments" | "statefulsets" | "daemonsets"
            ) {
                return Err(
                    "kubectl rollout resource must be deployments, statefulsets, or daemonsets"
                        .to_string(),
                );
            }
            let action = match subcommand {
                "status" => "rollout_status",
                "history" => "rollout_history",
                _ => {
                    return Err(format!(
                        "kubectl rollout subcommand is not allowed: {subcommand}"
                    ));
                }
            };
            Ok(RawKubectlPolicySubject::Resource {
                namespace,
                resource,
                name: Some(name),
                action: action.to_string(),
            })
        }
        Some(command) => Err(format!(
            "kubectl command is not allowed for raw diagnostic execution: {command}"
        )),
        None => Err("args must include at least one kubectl argument".to_string()),
    }
}

fn raw_kubectl_namespace(args: &[String]) -> Result<String, String> {
    let mut index = 0_usize;
    while index < args.len() {
        let arg = args[index].as_str();
        if matches!(arg, "-n" | "--namespace") {
            let namespace = args
                .get(index + 1)
                .ok_or_else(|| format!("kubectl flag requires a value: {arg}"))?;
            validate_kubernetes_dns_label(namespace, "namespace")?;
            return Ok(namespace.clone());
        }
        if let Some(namespace) = arg.strip_prefix("--namespace=") {
            validate_kubernetes_dns_label(namespace, "namespace")?;
            return Ok(namespace.to_string());
        }
        index += 1;
    }

    Err("resource kubectl diagnostics require -n/--namespace".to_string())
}

fn raw_kubectl_positionals(args: &[String], start: usize) -> Vec<String> {
    let mut positionals = Vec::new();
    let mut index = start;
    while index < args.len() {
        let arg = &args[index];
        if raw_kubectl_flag_takes_value(arg) {
            index += if arg.contains('=') { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        positionals.push(arg.clone());
        index += 1;
    }

    positionals
}

fn raw_kubectl_flag_takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "-n" | "--namespace" | "-o" | "--output" | "-l" | "--selector" | "--field-selector"
    ) || arg.starts_with("--namespace=")
        || arg.starts_with("--output=")
        || arg.starts_with("-o=")
        || arg.starts_with("--selector=")
        || arg.starts_with("--field-selector=")
}

fn raw_kubectl_resource_and_name(
    positionals: &[String],
) -> Result<(String, Option<String>), String> {
    let resource_token = positionals
        .first()
        .ok_or_else(|| "kubectl command requires a resource".to_string())?;
    if let Some((resource, name)) = resource_token.split_once('/') {
        if resource.is_empty() || name.is_empty() {
            return Err("kubectl resource/name must include both resource and name".to_string());
        }
        let resource = normalize_kubernetes_resource(resource)?;
        validate_kubernetes_dns_subdomain(name, "name")?;
        return Ok((resource, Some(name.to_string())));
    }

    let resource = normalize_kubernetes_resource(resource_token)?;
    let name = positionals.get(1).map(String::as_str);
    if let Some(name) = name {
        validate_kubernetes_dns_subdomain(name, "name")?;
    }

    Ok((resource, name.map(str::to_string)))
}

async fn execute_kubectl_read_for_source(
    store: &FileStore,
    source: &control_plane::SourceRef,
    args: &[String],
    timeout_seconds: u64,
    max_output_bytes: usize,
) -> Result<(KubectlRunOutput, Option<i64>), String> {
    let runtime = load_source_runtime_config(store, source, "kubernetes")
        .await?
        .ok_or_else(|| {
            "kubernetes source credential must be configured in the Gateway store file".to_string()
        })?;
    let credential_version = runtime.credential_version;
    if let Some(kubeconfig) = source_secret_string(&runtime, &["kubeconfig"]) {
        let path = std::env::temp_dir().join(format!(
            "action-gateway-kubeconfig-{}",
            uuid::Uuid::new_v4()
        ));
        fs::write(&path, kubeconfig)
            .await
            .map_err(|error| format!("failed to write source kubeconfig: {error}"))?;
        let result = execute_kubectl_read_with_kubeconfig(
            args,
            timeout_seconds,
            max_output_bytes,
            path.to_string_lossy().into_owned(),
        )
        .await;
        let _ = fs::remove_file(&path).await;
        return result.map(|output| (output, credential_version));
    }
    let kubeconfig = source_secret_string(
        &runtime,
        &["kubeconfigPath", "kubeconfig_path", "kubeconfigFile"],
    )
    .ok_or_else(|| {
        "kubernetes source credential must include kubeconfig or kubeconfigPath".to_string()
    })?;

    execute_kubectl_read_with_kubeconfig(args, timeout_seconds, max_output_bytes, kubeconfig)
        .await
        .map(|output| (output, credential_version))
}

async fn execute_kubectl_read_with_kubeconfig(
    args: &[String],
    timeout_seconds: u64,
    max_output_bytes: usize,
    kubeconfig: String,
) -> Result<KubectlRunOutput, String> {
    if kubeconfig.trim().is_empty() {
        return Err("kubernetes source kubeconfig must not be empty".to_string());
    }
    if kubeconfig.chars().any(char::is_control) {
        return Err("kubernetes source kubeconfig must not contain control characters".to_string());
    }

    let mut child = Command::new("kubectl")
        .args(args)
        .env("KUBECONFIG", kubeconfig)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("failed to spawn kubectl: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture kubectl stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture kubectl stderr".to_string())?;

    let stdout_task = tokio::spawn(read_stream_limited(stdout, max_output_bytes));
    let stderr_task = tokio::spawn(read_stream_limited(stderr, max_output_bytes));
    let mut timed_out = false;
    let status = match time::timeout(Duration::from_secs(timeout_seconds), child.wait()).await {
        Ok(Ok(status)) => Some(status),
        Ok(Err(error)) => return Err(format!("failed to wait for kubectl: {error}")),
        Err(_) => {
            timed_out = true;
            let _ = child.start_kill();
            let _ = child.wait().await;
            None
        }
    };
    let (stdout, stdout_truncated) = stdout_task
        .await
        .map_err(|error| format!("failed to join kubectl stdout reader: {error}"))?
        .map_err(|error| format!("failed to read kubectl stdout: {error}"))?;
    let (stderr, stderr_truncated) = stderr_task
        .await
        .map_err(|error| format!("failed to join kubectl stderr reader: {error}"))?
        .map_err(|error| format!("failed to read kubectl stderr: {error}"))?;

    Ok(KubectlRunOutput {
        exit_code: status.and_then(|status| status.code()),
        timed_out,
        stdout: output_bytes_to_string(stdout),
        stderr: output_bytes_to_string(stderr),
        stdout_truncated,
        stderr_truncated,
    })
}

async fn read_stream_limited<R>(
    mut reader: R,
    max_output_bytes: usize,
) -> Result<(Vec<u8>, bool), std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }

        let remaining = max_output_bytes.saturating_sub(output.len());
        if remaining == 0 {
            truncated = true;
            continue;
        }

        let bytes_to_store = remaining.min(bytes_read);
        output.extend_from_slice(&buffer[..bytes_to_store]);
        if bytes_to_store < bytes_read {
            truncated = true;
        }
    }

    Ok((output, truncated))
}

fn output_bytes_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

async fn query_redis_key(store: &FileStore, redis: &RedisClient, arguments: &Value) -> Value {
    if missing_string(arguments, "key") {
        return tool_argument_error(
            TOOL_QUERY_REDIS_KEY,
            "missing required argument: key",
            arguments,
        );
    }

    let source_ref = match source_ref_from_tool_arguments(arguments) {
        Ok(source_ref) => source_ref,
        Err(message) => return tool_argument_error(TOOL_QUERY_REDIS_KEY, &message, arguments),
    };
    let key = arguments
        .get("key")
        .and_then(Value::as_str)
        .expect("key was checked above");
    if let Err(message) = validate_redis_key_argument(key) {
        return tool_argument_error(TOOL_QUERY_REDIS_KEY, &message, arguments);
    }

    let allowlist = match load_allowed_redis_key(store, &source_ref, key).await {
        Ok(Some(allowlist)) => allowlist,
        Ok(None) => {
            return tool_error_result(
                TOOL_QUERY_REDIS_KEY,
                "not_allowed",
                "redis key is not allowlisted",
                arguments,
            );
        }
        Err(message) => {
            return tool_error_result(
                TOOL_QUERY_REDIS_KEY,
                "query_failed",
                &format!("failed to load redis key allowlist: {message}"),
                arguments,
            );
        }
    };
    let limit = match parse_redis_query_limit(arguments, allowlist.max_members) {
        Ok(limit) => limit,
        Err(message) => return tool_argument_error(TOOL_QUERY_REDIS_KEY, &message, arguments),
    };

    let (source_redis, credential_version) =
        match redis_client_for_source(store, &source_ref, "redis").await {
            Ok((Some(source_redis), credential_version)) => (source_redis, credential_version),
            Ok((None, credential_version)) => (redis.clone(), credential_version),
            Err(message) => {
                return tool_error_result(
                    TOOL_QUERY_REDIS_KEY,
                    "query_failed",
                    &message,
                    arguments,
                );
            }
        };

    match execute_redis_key_query(&source_redis, key, &allowlist, limit).await {
        Ok(read) => redis_key_query_result(
            &source_ref,
            key,
            allowlist,
            limit,
            read,
            credential_version,
            arguments,
        ),
        Err(message) => tool_error_result(
            TOOL_QUERY_REDIS_KEY,
            "query_failed",
            &format!("redis key query failed: {message}"),
            arguments,
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AllowedRedisKey {
    pattern: String,
    max_value_bytes: usize,
    max_members: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct RedisKeyRead {
    key_type: String,
    ttl_seconds: i64,
    data: Value,
}

async fn load_allowed_redis_key(
    store: &FileStore,
    source: &control_plane::SourceRef,
    key: &str,
) -> Result<Option<AllowedRedisKey>, String> {
    for record in store.redis_key_allowlist(&source.source_name).await {
        let pattern = record.key_pattern;
        let regex = Regex::new(&pattern)
            .map_err(|error| format!("invalid regex pattern {pattern}: {error}"))?;
        if !regex_matches_entire_key(&regex, key) {
            continue;
        }

        return Ok(Some(AllowedRedisKey {
            pattern,
            max_value_bytes: record.max_value_bytes.max(1),
            max_members: record.max_members.clamp(1, 1000),
        }));
    }

    Ok(None)
}

fn regex_matches_entire_key(regex: &Regex, key: &str) -> bool {
    regex
        .find(key)
        .is_some_and(|matched| matched.start() == 0 && matched.end() == key.len())
}

fn validate_redis_key_argument(key: &str) -> Result<(), String> {
    if key.len() > 1024 {
        return Err("key must be 1024 bytes or fewer".to_string());
    }
    if key.chars().any(char::is_control) {
        return Err("key must not contain control characters".to_string());
    }

    Ok(())
}

fn parse_redis_query_limit(
    arguments: &Value,
    allowlist_max_members: usize,
) -> Result<usize, String> {
    let configured_max = allowlist_max_members.clamp(1, 1000);
    let limit = match arguments.get("limit") {
        Some(Value::Number(number)) => number
            .as_u64()
            .ok_or_else(|| "limit must be a positive integer".to_string())?,
        Some(_) => return Err("limit must be a positive integer".to_string()),
        None => configured_max.min(100) as u64,
    };

    if limit < 1 {
        return Err("limit must be a positive integer".to_string());
    }
    if limit > configured_max as u64 {
        return Err(format!(
            "limit must be less than or equal to {configured_max}"
        ));
    }

    usize::try_from(limit).map_err(|_| "limit is too large".to_string())
}

async fn execute_redis_key_query(
    redis: &RedisClient,
    key: &str,
    allowlist: &AllowedRedisKey,
    limit: usize,
) -> Result<RedisKeyRead, String> {
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|error| error.to_string())?;
    let key_type = redis::cmd("TYPE")
        .arg(key)
        .query_async::<String>(&mut connection)
        .await
        .map_err(|error| error.to_string())?;
    let ttl_seconds = redis::cmd("TTL")
        .arg(key)
        .query_async::<i64>(&mut connection)
        .await
        .map_err(|error| error.to_string())?;

    let data = match key_type.as_str() {
        "none" => json!({
            "exists": false
        }),
        "string" => {
            let value_length = redis::cmd("STRLEN")
                .arg(key)
                .query_async::<usize>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            if value_length > allowlist.max_value_bytes {
                return Err(format!(
                    "redis string value is {value_length} byte(s), exceeding max_value_bytes {}",
                    allowlist.max_value_bytes
                ));
            }

            let value = redis::cmd("GET")
                .arg(key)
                .query_async::<Vec<u8>>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;

            json!({
                "exists": true,
                "valueLength": value_length,
                "value": redis_bytes_to_json_with_limit(
                    value,
                    allowlist.max_value_bytes,
                    "string value"
                )?
            })
        }
        "hash" => {
            let field_count = redis::cmd("HLEN")
                .arg(key)
                .query_async::<usize>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            let (cursor, entries) = redis::cmd("HSCAN")
                .arg(key)
                .arg(0)
                .arg("COUNT")
                .arg(limit)
                .query_async::<(u64, Vec<(Vec<u8>, Vec<u8>)>)>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            let scanned_count = entries.len();
            let mut entries_json = serde_json::Map::new();
            for (field, value) in entries.into_iter().take(limit) {
                entries_json.insert(
                    redis_bytes_to_display_string_with_limit(
                        field,
                        allowlist.max_value_bytes,
                        "hash field",
                    )?,
                    redis_bytes_to_json_with_limit(value, allowlist.max_value_bytes, "hash value")?,
                );
            }
            let entries = entries_json;
            let returned_count = entries.len();

            json!({
                "exists": true,
                "fieldCount": field_count,
                "returnedCount": returned_count,
                "truncated": cursor != 0 || field_count > returned_count || scanned_count > returned_count,
                "entries": entries
            })
        }
        "list" => {
            let member_count = redis::cmd("LLEN")
                .arg(key)
                .query_async::<usize>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            let stop = limit.saturating_sub(1);
            let members = redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(stop)
                .query_async::<Vec<Vec<u8>>>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            let mut members_json = Vec::with_capacity(members.len());
            for member in members {
                members_json.push(redis_bytes_to_json_with_limit(
                    member,
                    allowlist.max_value_bytes,
                    "list member",
                )?);
            }
            let members = members_json;

            json!({
                "exists": true,
                "memberCount": member_count,
                "returnedCount": members.len(),
                "truncated": member_count > members.len(),
                "members": members
            })
        }
        "set" => {
            let member_count = redis::cmd("SCARD")
                .arg(key)
                .query_async::<usize>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            let (cursor, members) = redis::cmd("SSCAN")
                .arg(key)
                .arg(0)
                .arg("COUNT")
                .arg(limit)
                .query_async::<(u64, Vec<Vec<u8>>)>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            let scanned_count = members.len();
            let mut members_json = Vec::new();
            for member in members.into_iter().take(limit) {
                members_json.push(redis_bytes_to_json_with_limit(
                    member,
                    allowlist.max_value_bytes,
                    "set member",
                )?);
            }
            let members = members_json;
            let returned_count = members.len();

            json!({
                "exists": true,
                "memberCount": member_count,
                "returnedCount": returned_count,
                "truncated": cursor != 0 || member_count > returned_count || scanned_count > returned_count,
                "members": members
            })
        }
        "zset" => {
            let member_count = redis::cmd("ZCARD")
                .arg(key)
                .query_async::<usize>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            let stop = limit.saturating_sub(1);
            let members = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(stop)
                .arg("WITHSCORES")
                .query_async::<Vec<(Vec<u8>, f64)>>(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
            let mut members_json = Vec::with_capacity(members.len());
            for (member, score) in members {
                members_json.push(json!({
                    "member": redis_bytes_to_json_with_limit(
                        member,
                        allowlist.max_value_bytes,
                        "zset member"
                    )?,
                    "score": score
                }));
            }
            let members = members_json;

            json!({
                "exists": true,
                "memberCount": member_count,
                "returnedCount": members.len(),
                "truncated": member_count > members.len(),
                "members": members
            })
        }
        _ => {
            return Err(format!("unsupported redis key type: {key_type}"));
        }
    };

    Ok(RedisKeyRead {
        key_type,
        ttl_seconds,
        data,
    })
}

fn redis_bytes_to_json_with_limit(
    bytes: Vec<u8>,
    max_value_bytes: usize,
    label: &str,
) -> Result<Value, String> {
    if bytes.len() > max_value_bytes {
        return Err(format!(
            "redis {label} is {} byte(s), exceeding max_value_bytes {max_value_bytes}",
            bytes.len()
        ));
    }

    Ok(redis_bytes_to_json(bytes))
}

fn redis_bytes_to_display_string_with_limit(
    bytes: Vec<u8>,
    max_value_bytes: usize,
    label: &str,
) -> Result<String, String> {
    if bytes.len() > max_value_bytes {
        return Err(format!(
            "redis {label} is {} byte(s), exceeding max_value_bytes {max_value_bytes}",
            bytes.len()
        ));
    }

    Ok(redis_bytes_to_display_string(bytes))
}

fn redis_bytes_to_json(bytes: Vec<u8>) -> Value {
    match String::from_utf8(bytes) {
        Ok(value) => Value::String(value),
        Err(error) => json!({
            "encoding": "hex",
            "value": hex_encode(error.as_bytes())
        }),
    }
}

fn redis_bytes_to_display_string(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(value) => value,
        Err(error) => format!("0x{}", hex_encode(error.as_bytes())),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn missing_string(arguments: &Value, name: &str) -> bool {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
}

fn source_ref_from_tool_arguments(arguments: &Value) -> Result<control_plane::SourceRef, String> {
    control_plane::source_ref_from_arguments(arguments, &control_plane::AuthContext::legacy_admin())
}

#[derive(Debug, Clone)]
struct SourceRuntimeConfig {
    config: Value,
    credential: Option<Value>,
    credential_version: Option<i64>,
}

async fn mysql_pool_for_source(
    store: &FileStore,
    source: &control_plane::SourceRef,
) -> Result<(MySqlPool, Option<i64>), String> {
    let runtime = load_source_runtime_config(store, source, "mysql")
        .await?
        .ok_or_else(|| {
            "mysql source credential must be configured in the Gateway store file".to_string()
        })?;
    let url = source_secret_string(&runtime, &["url", "connectionUrl", "databaseUrl"])
        .ok_or_else(|| "mysql source credential must include url".to_string())?;
    let pool = MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .map_err(|error| format!("failed to connect mysql source: {error}"))?;

    Ok((pool, runtime.credential_version))
}

async fn redis_client_for_source(
    store: &FileStore,
    source: &control_plane::SourceRef,
    source_type: &str,
) -> Result<(Option<RedisClient>, Option<i64>), String> {
    let Some(runtime) = load_source_runtime_config(store, source, source_type).await? else {
        return Ok((None, None));
    };
    let Some(url) = source_secret_string(&runtime, &["url", "connectionUrl", "redisUrl"]) else {
        return Err(format!("{source_type} source credential must include url"));
    };
    let client =
        RedisClient::open(url).map_err(|error| format!("failed to open redis source: {error}"))?;

    Ok((Some(client), runtime.credential_version))
}

async fn load_source_runtime_config(
    store: &FileStore,
    source: &control_plane::SourceRef,
    source_type: &str,
) -> Result<Option<SourceRuntimeConfig>, String> {
    let Some(record) = store.source(&source.source_name, source_type).await else {
        return Ok(None);
    };

    Ok(Some(SourceRuntimeConfig {
        config: record.config,
        credential: record.credential,
        credential_version: record.credential_version,
    }))
}

fn source_secret_string(runtime: &SourceRuntimeConfig, keys: &[&str]) -> Option<String> {
    if let Some(value) = source_secret_string_from(runtime.credential.as_ref(), keys) {
        return Some(value);
    }
    source_secret_string_from(Some(&runtime.config), keys)
}

fn source_secret_string_from(source: Option<&Value>, keys: &[&str]) -> Option<String> {
    let source = source?;
    for key in keys {
        if let Some(value) = source.get(*key).and_then(Value::as_str)
            && !value.trim().is_empty()
        {
            return Some(value.to_string());
        }
    }

    None
}

#[derive(Debug)]
struct AllowedTable {
    columns: Vec<String>,
    max_limit: i64,
    max_estimated_rows: i64,
    mask_rules: Value,
}

#[derive(Debug, PartialEq, Eq)]
struct TablePath {
    schema: Option<String>,
    table: String,
    quoted: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderBy {
    column: String,
    direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }

    fn as_json(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

async fn load_allowed_table(
    store: &FileStore,
    source: &control_plane::SourceRef,
    table_name: &str,
) -> Option<AllowedTable> {
    store
        .table_allowlist(&source.source_name, table_name)
        .await
        .map(|record| AllowedTable {
            columns: record.columns,
            max_limit: record.max_limit,
            max_estimated_rows: record.max_estimated_rows,
            mask_rules: record.mask_rules,
        })
}

async fn load_table_columns(
    db: &MySqlPool,
    table_path: &TablePath,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = if let Some(schema) = &table_path.schema {
        sqlx::query_as::<_, (String,)>(
            "SELECT COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(&table_path.table)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as::<_, (String,)>(
            "SELECT COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(&table_path.table)
        .fetch_all(db)
        .await?
    };

    Ok(rows.into_iter().map(|(column,)| column).collect())
}

fn parse_table_path(table_name: &str) -> Result<TablePath, String> {
    let parts = table_name.split('.').collect::<Vec<_>>();

    match parts.as_slice() {
        [table] if is_valid_identifier(table) => Ok(TablePath {
            schema: None,
            table: (*table).to_string(),
            quoted: quote_identifier(table),
        }),
        [schema, table] if is_valid_identifier(schema) && is_valid_identifier(table) => {
            Ok(TablePath {
                schema: Some((*schema).to_string()),
                table: (*table).to_string(),
                quoted: format!("{}.{}", quote_identifier(schema), quote_identifier(table)),
            })
        }
        _ => Err("table_name must be a valid MySQL identifier or schema.table path".to_string()),
    }
}

fn parse_columns_argument(arguments: &Value) -> Result<Option<Vec<String>>, String> {
    let Some(columns) = arguments.get("columns") else {
        return Ok(None);
    };
    let columns = columns
        .as_array()
        .ok_or_else(|| "columns must be an array of strings".to_string())?;
    let mut parsed = Vec::with_capacity(columns.len());

    for column in columns {
        let column = column
            .as_str()
            .ok_or_else(|| "columns must be an array of strings".to_string())?;
        parsed.push(column.to_string());
    }

    normalize_columns(parsed, "column").map(Some)
}

fn parse_filters_argument(
    arguments: &Value,
    allowed_columns: &[String],
    actual_columns: &[String],
) -> Result<Vec<(String, Value)>, String> {
    let Some(filters) = arguments.get("filters") else {
        return Ok(Vec::new());
    };
    let filters = filters
        .as_object()
        .ok_or_else(|| "filters must be an object".to_string())?;
    let allowed_set = columns_set(allowed_columns);
    let actual_set = columns_set(actual_columns);
    let mut parsed = Vec::with_capacity(filters.len());

    for (column, value) in filters {
        if !is_valid_identifier(column) {
            return Err(format!("filter column is not a valid identifier: {column}"));
        }
        if !actual_set.contains(column) {
            return Err(format!("filter column does not exist on table: {column}"));
        }
        if !allowed_set.is_empty() && !allowed_set.contains(column) {
            return Err(format!("filter column is not allowlisted: {column}"));
        }
        if !is_supported_filter_value(value) {
            return Err(format!(
                "filter value for column {column} must be string, number, boolean, or null"
            ));
        }

        parsed.push((column.to_string(), value.clone()));
    }

    parsed.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(parsed)
}

const MAX_ORDER_BY_COLUMNS: usize = 3;

fn parse_order_by_argument(
    arguments: &Value,
    allowed_columns: &[String],
    actual_columns: &[String],
) -> Result<Vec<OrderBy>, String> {
    let Some(order_by) = arguments.get("order_by") else {
        return Ok(Vec::new());
    };
    let order_by = order_by
        .as_array()
        .ok_or_else(|| "order_by must be an array".to_string())?;
    if order_by.len() > MAX_ORDER_BY_COLUMNS {
        return Err(format!(
            "order_by must include at most {MAX_ORDER_BY_COLUMNS} column(s)"
        ));
    }

    let allowed_set = columns_set(allowed_columns);
    let actual_set = columns_set(actual_columns);
    let mut seen = HashSet::new();
    let mut parsed = Vec::with_capacity(order_by.len());

    for item in order_by {
        let item = item
            .as_object()
            .ok_or_else(|| "order_by entries must be objects".to_string())?;
        let column = item
            .get("column")
            .and_then(Value::as_str)
            .ok_or_else(|| "order_by entry must include column".to_string())?;
        if !is_valid_identifier(column) {
            return Err(format!(
                "order_by column is not a valid identifier: {column}"
            ));
        }
        if !actual_set.contains(column) {
            return Err(format!("order_by column does not exist on table: {column}"));
        }
        if !allowed_set.is_empty() && !allowed_set.contains(column) {
            return Err(format!("order_by column is not allowlisted: {column}"));
        }
        if !seen.insert(column.to_string()) {
            return Err(format!("duplicate order_by column: {column}"));
        }

        let direction = match item.get("direction") {
            Some(Value::String(direction)) if direction.eq_ignore_ascii_case("asc") => {
                SortDirection::Asc
            }
            Some(Value::String(direction)) if direction.eq_ignore_ascii_case("desc") => {
                SortDirection::Desc
            }
            Some(Value::String(_)) => {
                return Err("order_by direction must be asc or desc".to_string());
            }
            Some(_) => return Err("order_by direction must be a string".to_string()),
            None => SortDirection::Asc,
        };

        parsed.push(OrderBy {
            column: column.to_string(),
            direction,
        });
    }

    Ok(parsed)
}

fn order_by_to_json(order_by: &[OrderBy]) -> Vec<Value> {
    order_by
        .iter()
        .map(|item| {
            json!({
                "column": &item.column,
                "direction": item.direction.as_json()
            })
        })
        .collect()
}

fn parse_limit(arguments: &Value, allowlist_max_limit: i64) -> Result<i64, String> {
    let configured_max = allowlist_max_limit.clamp(1, 1000);
    let limit = match arguments.get("limit") {
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| "limit must be a positive integer".to_string())?,
        Some(_) => return Err("limit must be a positive integer".to_string()),
        None => 100.min(configured_max),
    };

    if limit < 1 {
        return Err("limit must be a positive integer".to_string());
    }
    if limit > configured_max {
        return Err(format!(
            "limit must be less than or equal to {configured_max}"
        ));
    }

    Ok(limit)
}

fn select_columns(
    requested_columns: Option<Vec<String>>,
    allowed_columns: &[String],
    actual_columns: &[String],
) -> Result<Vec<String>, String> {
    let actual_set = columns_set(actual_columns);
    let allowed_set = columns_set(allowed_columns);
    let selected = match requested_columns {
        Some(columns) => columns,
        None if !allowed_columns.is_empty() => allowed_columns.to_vec(),
        None => actual_columns.to_vec(),
    };

    if selected.is_empty() {
        return Err("at least one column must be selected".to_string());
    }

    for column in &selected {
        if !actual_set.contains(column) {
            return Err(format!("column does not exist on table: {column}"));
        }
        if !allowed_set.is_empty() && !allowed_set.contains(column) {
            return Err(format!("column is not allowlisted: {column}"));
        }
    }

    Ok(selected)
}

fn normalize_columns(columns: Vec<String>, label: &str) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(columns.len());

    for column in columns {
        if !is_valid_identifier(&column) {
            return Err(format!("{label} is not a valid identifier: {column}"));
        }
        if seen.insert(column.clone()) {
            normalized.push(column);
        }
    }

    Ok(normalized)
}

fn columns_set(columns: &[String]) -> HashSet<String> {
    columns.iter().cloned().collect()
}

fn is_supported_filter_value(value: &Value) -> bool {
    value.is_null() || value.is_boolean() || value.is_number() || value.is_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldMaskRule {
    strategy: MaskStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MaskStrategy {
    Full {
        replacement: String,
    },
    Partial {
        prefix: usize,
        suffix: usize,
        mask: String,
    },
    Email {
        mask: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct MaskingReport {
    applied: bool,
    masked_columns: Vec<String>,
}

fn parse_mask_rules(
    mask_rules: Value,
    allowed_columns: &[String],
    actual_columns: &[String],
) -> Result<Vec<(String, FieldMaskRule)>, String> {
    let mask_rules = mask_rules
        .as_object()
        .ok_or_else(|| "mask_rules must be a JSON object".to_string())?;
    let allowed_set = columns_set(allowed_columns);
    let actual_set = columns_set(actual_columns);
    let mut parsed = Vec::with_capacity(mask_rules.len());

    for (column, rule) in mask_rules {
        if !is_valid_identifier(column) {
            return Err(format!(
                "mask rule column is not a valid identifier: {column}"
            ));
        }
        if !actual_set.contains(column) {
            return Err(format!(
                "mask rule column does not exist on table: {column}"
            ));
        }
        if !allowed_set.is_empty() && !allowed_set.contains(column) {
            return Err(format!("mask rule column is not allowlisted: {column}"));
        }

        parsed.push((column.to_string(), parse_mask_rule(rule, column)?));
    }

    parsed.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(parsed)
}

fn parse_mask_rule(rule: &Value, column: &str) -> Result<FieldMaskRule, String> {
    let strategy = match rule {
        Value::String(strategy) => parse_mask_strategy(strategy, None, column)?,
        Value::Object(object) => {
            let strategy = object
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("mask rule for column {column} must include type"))?;
            parse_mask_strategy(strategy, Some(object), column)?
        }
        _ => {
            return Err(format!(
                "mask rule for column {column} must be a string or object"
            ));
        }
    };

    Ok(FieldMaskRule { strategy })
}

fn parse_mask_strategy(
    strategy: &str,
    object: Option<&serde_json::Map<String, Value>>,
    column: &str,
) -> Result<MaskStrategy, String> {
    match strategy {
        "full" => Ok(MaskStrategy::Full {
            replacement: optional_rule_string(object, "replacement", "***", column)?,
        }),
        "partial" => Ok(MaskStrategy::Partial {
            prefix: optional_rule_usize(object, "prefix", 0, column)?,
            suffix: optional_rule_usize(object, "suffix", 4, column)?,
            mask: optional_rule_string(object, "mask", "*", column)?,
        }),
        "email" => Ok(MaskStrategy::Email {
            mask: optional_rule_string(object, "mask", "*", column)?,
        }),
        _ => Err(format!(
            "mask rule for column {column} has unsupported type: {strategy}"
        )),
    }
}

fn optional_rule_string(
    object: Option<&serde_json::Map<String, Value>>,
    name: &str,
    default: &str,
    column: &str,
) -> Result<String, String> {
    let Some(value) = object.and_then(|object| object.get(name)) else {
        return Ok(default.to_string());
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("mask rule {name} for column {column} must be a string"))?;

    if value.is_empty() {
        return Err(format!(
            "mask rule {name} for column {column} must not be empty"
        ));
    }

    Ok(value.to_string())
}

fn optional_rule_usize(
    object: Option<&serde_json::Map<String, Value>>,
    name: &str,
    default: usize,
    column: &str,
) -> Result<usize, String> {
    let Some(value) = object.and_then(|object| object.get(name)) else {
        return Ok(default);
    };
    let value = value.as_u64().ok_or_else(|| {
        format!("mask rule {name} for column {column} must be a non-negative integer")
    })?;

    usize::try_from(value).map_err(|_| format!("mask rule {name} for column {column} is too large"))
}

fn mask_rows(
    mut rows: Vec<Value>,
    selected_columns: &[String],
    mask_rules: &[(String, FieldMaskRule)],
) -> (Vec<Value>, MaskingReport) {
    let selected_set = columns_set(selected_columns);
    let masked_columns = mask_rules
        .iter()
        .filter_map(|(column, _)| selected_set.contains(column).then_some(column.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    if masked_columns.is_empty() {
        return (
            rows,
            MaskingReport {
                applied: false,
                masked_columns,
            },
        );
    }

    for row in &mut rows {
        let Some(object) = row.as_object_mut() else {
            continue;
        };

        for (column, rule) in mask_rules {
            if !selected_set.contains(column) {
                continue;
            }
            let Some(value) = object.get_mut(column) else {
                continue;
            };

            mask_value(value, rule);
        }
    }

    (
        rows,
        MaskingReport {
            applied: true,
            masked_columns,
        },
    )
}

fn mask_value(value: &mut Value, rule: &FieldMaskRule) {
    if value.is_null() {
        return;
    }

    let original = match value {
        Value::String(value) => value.clone(),
        Value::Bool(_) | Value::Number(_) => value.to_string(),
        Value::Array(_) | Value::Object(_) | Value::Null => return,
    };

    let masked = match &rule.strategy {
        MaskStrategy::Full { replacement } => replacement.clone(),
        MaskStrategy::Partial {
            prefix,
            suffix,
            mask,
        } => mask_partial(&original, *prefix, *suffix, mask),
        MaskStrategy::Email { mask } => mask_email(&original, mask),
    };

    *value = Value::String(masked);
}

fn mask_email(value: &str, mask: &str) -> String {
    let Some((local, domain)) = value.split_once('@') else {
        return mask_partial(value, 1, 0, mask);
    };

    format!("{}@{domain}", mask_partial(local, 1, 0, mask))
}

fn mask_partial(value: &str, prefix: usize, suffix: usize, mask: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let length = characters.len();

    if length == 0 {
        return String::new();
    }
    if prefix.saturating_add(suffix) >= length {
        return mask.repeat(length);
    }

    let visible_prefix = characters.iter().take(prefix).collect::<String>();
    let visible_suffix = characters.iter().skip(length - suffix).collect::<String>();
    let masked_length = length - prefix - suffix;

    format!(
        "{visible_prefix}{}{visible_suffix}",
        mask.repeat(masked_length)
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExplainGateReport {
    max_estimated_rows: u64,
    estimated_rows: u64,
    passed: bool,
    plan: Vec<ExplainPlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExplainPlanStep {
    #[serde(skip_serializing_if = "Option::is_none")]
    select_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    possible_keys: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    estimated_rows: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<String>,
}

async fn explain_table_query(
    db: &MySqlPool,
    table_path: &TablePath,
    columns: &[String],
    filters: &[(String, Value)],
    order_by: &[OrderBy],
    limit: i64,
    max_estimated_rows: i64,
) -> Result<ExplainGateReport, sqlx::Error> {
    if max_estimated_rows < 1 {
        return Err(decode_error("max_estimated_rows must be positive"));
    }

    let max_estimated_rows = u64::try_from(max_estimated_rows)
        .map_err(|_| decode_error("max_estimated_rows must be positive"))?;
    let mut builder = QueryBuilder::<MySql>::new("EXPLAIN ");
    append_table_query(&mut builder, table_path, columns, filters, order_by, limit)?;
    let rows = builder.build().fetch_all(db).await?;

    if rows.is_empty() {
        return Err(decode_error("EXPLAIN returned no plan rows"));
    }

    let mut estimated_rows = 0_u64;
    let mut plan = Vec::with_capacity(rows.len());

    for row in rows {
        let step_estimated_rows = explain_optional_u64(&row, "rows", 9)?
            .ok_or_else(|| decode_error("EXPLAIN plan row is missing estimated rows"))?;

        estimated_rows = estimated_rows.saturating_add(step_estimated_rows);
        plan.push(ExplainPlanStep {
            select_type: explain_optional_string(&row, "select_type", 1)?,
            table: explain_optional_string(&row, "table", 2)?,
            access_type: explain_optional_string(&row, "type", 4)?,
            possible_keys: explain_optional_string(&row, "possible_keys", 5)?,
            key: explain_optional_string(&row, "key", 6)?,
            estimated_rows: step_estimated_rows,
            extra: explain_optional_string(&row, "Extra", 11)?,
        });
    }

    Ok(ExplainGateReport {
        max_estimated_rows,
        estimated_rows,
        passed: estimated_rows <= max_estimated_rows,
        plan,
    })
}

fn explain_optional_string(
    row: &MySqlRow,
    name: &'static str,
    index: usize,
) -> Result<Option<String>, sqlx::Error> {
    row.try_get::<Option<String>, _>(name)
        .or_else(|error| match error {
            sqlx::Error::ColumnNotFound(_) => row.try_get(index),
            error => Err(error),
        })
}

fn explain_optional_u64(
    row: &MySqlRow,
    name: &'static str,
    index: usize,
) -> Result<Option<u64>, sqlx::Error> {
    match row.try_get::<Option<u64>, _>(name) {
        Ok(value) => Ok(value),
        Err(sqlx::Error::ColumnNotFound(_)) => row.try_get(index),
        Err(error) => Err(error),
    }
}

async fn execute_table_query(
    db: &MySqlPool,
    table_path: &TablePath,
    columns: &[String],
    filters: &[(String, Value)],
    order_by: &[OrderBy],
    limit: i64,
) -> Result<Vec<Value>, sqlx::Error> {
    let mut builder = QueryBuilder::<MySql>::new("");
    append_table_query(&mut builder, table_path, columns, filters, order_by, limit)?;
    let rows = builder.build().fetch_all(db).await?;
    let mut json_rows = Vec::with_capacity(rows.len());

    for row in rows {
        let row_json = row.try_get::<Json<Value>, _>("row_json")?;
        json_rows.push(row_json.0);
    }

    Ok(json_rows)
}

fn append_table_query<'args>(
    builder: &mut QueryBuilder<'args, MySql>,
    table_path: &TablePath,
    columns: &'args [String],
    filters: &'args [(String, Value)],
    order_by: &'args [OrderBy],
    limit: i64,
) -> Result<(), sqlx::Error> {
    builder.push("SELECT JSON_OBJECT(");

    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }

        builder.push("'");
        builder.push(column);
        builder.push("', ");
        builder.push(quote_identifier(column));
    }

    builder.push(") AS row_json FROM ");
    builder.push(&table_path.quoted);

    if !filters.is_empty() {
        builder.push(" WHERE ");

        for (index, (column, value)) in filters.iter().enumerate() {
            if index > 0 {
                builder.push(" AND ");
            }

            builder.push(quote_identifier(column));
            builder.push(" <=> ");
            push_json_value_bind(builder, value)?;
        }
    }

    if !order_by.is_empty() {
        builder.push(" ORDER BY ");

        for (index, item) in order_by.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }

            builder.push(quote_identifier(&item.column));
            builder.push(" ");
            builder.push(item.direction.as_sql());
        }
    }

    builder.push(" LIMIT ");
    builder.push_bind(limit);

    Ok(())
}

fn push_json_value_bind<'args>(
    builder: &mut QueryBuilder<'args, MySql>,
    value: &'args Value,
) -> Result<(), sqlx::Error> {
    match value {
        Value::Null => {
            builder.push_bind(Option::<String>::None);
        }
        Value::Bool(value) => {
            builder.push_bind(*value);
        }
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                builder.push_bind(value);
            } else if let Some(value) = value.as_u64() {
                builder.push_bind(value);
            } else if let Some(value) = value.as_f64() {
                builder.push_bind(value);
            } else {
                return Err(decode_error("unsupported JSON number filter value"));
            }
        }
        Value::String(value) => {
            builder.push_bind(value.as_str());
        }
        Value::Array(_) | Value::Object(_) => {
            return Err(decode_error("unsupported JSON filter value type"));
        }
    }

    Ok(())
}

fn decode_error(message: &'static str) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    )))
}

#[allow(clippy::too_many_arguments)]
fn table_query_result(
    source: &control_plane::SourceRef,
    table_name: &str,
    columns: Vec<String>,
    filters: Vec<(String, Value)>,
    order_by: Vec<OrderBy>,
    limit: i64,
    rows: Vec<Value>,
    explain_gate: ExplainGateReport,
    masking: MaskingReport,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    let row_count = rows.len();
    let filters = filters.into_iter().collect::<serde_json::Map<_, _>>();
    let order_by = order_by_to_json(&order_by);

    json!({
        "content": [
            {
                "type": "text",
                "text": format!("{TOOL_QUERY_TABLE_DATA}: returned {row_count} row(s) from {table_name}")
            }
        ],
        "structuredContent": {
            "status": "succeeded",
            "action": TOOL_QUERY_TABLE_DATA,
            "sourceName": source.source_name,
            "credentialVersion": credential_version,
            "tableName": table_name,
            "columns": columns,
            "filters": filters,
            "orderBy": order_by,
            "limit": limit,
            "rowCount": row_count,
            "rows": rows,
            "explainGate": explain_gate,
            "masking": masking,
            "receivedArguments": arguments
        },
        "isError": false
    })
}

fn redis_key_query_result(
    source: &control_plane::SourceRef,
    key: &str,
    allowlist: AllowedRedisKey,
    limit: usize,
    read: RedisKeyRead,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    let mut structured = json!({
        "status": "succeeded",
        "action": TOOL_QUERY_REDIS_KEY,
        "sourceName": source.source_name,
        "credentialVersion": credential_version,
        "key": key,
        "keyType": read.key_type,
        "ttlSeconds": read.ttl_seconds,
        "limit": limit,
        "allowlist": {
            "matchedPattern": allowlist.pattern,
            "maxValueBytes": allowlist.max_value_bytes,
            "maxMembers": allowlist.max_members
        },
        "receivedArguments": arguments
    });
    if let (Some(structured), Some(data)) = (structured.as_object_mut(), read.data.as_object()) {
        for (name, value) in data {
            structured.insert(name.clone(), value.clone());
        }
    }

    json!({
        "content": [
            {
                "type": "text",
                "text": format!(
                    "{TOOL_QUERY_REDIS_KEY}: returned key {key} ({})",
                    structured["keyType"].as_str().unwrap_or("unknown")
                )
            }
        ],
        "structuredContent": structured,
        "isError": false
    })
}

fn kubernetes_list_result(
    query: KubernetesListQuery,
    policy: KubernetesResourcePolicy,
    limit: usize,
    kubectl_args: Vec<String>,
    output: KubectlRunOutput,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    let succeeded = kubectl_succeeded(&output);
    if !succeeded {
        return kubernetes_command_result(
            TOOL_LIST_KUBERNETES_RESOURCES,
            format!(
                "kubectl get {} in namespace {} {}",
                query.resource,
                query.namespace,
                kubectl_exit_summary(&output, DEFAULT_KUBECTL_TIMEOUT_SECONDS)
            ),
            query.source.clone(),
            query.namespace,
            query.resource,
            None,
            kubectl_args,
            DEFAULT_KUBECTL_TIMEOUT_SECONDS,
            policy.max_output_bytes,
            Some(policy),
            output,
            credential_version,
            arguments,
        );
    }

    let parsed = match parse_kubectl_json_output(&output) {
        Ok(parsed) => parsed,
        Err(message) => {
            return kubernetes_json_error_result(
                TOOL_LIST_KUBERNETES_RESOURCES,
                &message,
                query.source.clone(),
                query.namespace,
                query.resource,
                None,
                kubectl_args,
                policy.max_output_bytes,
                Some(policy),
                output,
                credential_version,
                arguments,
            );
        }
    };
    let items = parsed
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total_items = items.len();
    let summaries = items
        .iter()
        .take(limit)
        .map(|item| summarize_kubernetes_resource(&query.resource, item))
        .collect::<Vec<_>>();
    let returned_count = summaries.len();
    let truncated = total_items > returned_count || output.stdout_truncated;
    let command = kubectl_command(&kubectl_args);

    json!({
        "content": [
            {
                "type": "text",
                "text": format!(
                    "{TOOL_LIST_KUBERNETES_RESOURCES}: returned {returned_count} {} resource(s) from namespace {}",
                    query.resource,
                    query.namespace
                )
            }
        ],
        "structuredContent": {
            "status": "succeeded",
            "action": TOOL_LIST_KUBERNETES_RESOURCES,
            "sourceName": query.source.source_name,
            "credentialVersion": credential_version,
            "namespace": query.namespace,
            "resource": query.resource,
            "labelSelector": query.label_selector,
            "fieldSelector": query.field_selector,
            "limit": limit,
            "totalItems": total_items,
            "returnedCount": returned_count,
            "truncated": truncated,
            "items": summaries,
            "command": command,
            "exitCode": output.exit_code,
            "timedOut": output.timed_out,
            "timeoutSeconds": DEFAULT_KUBECTL_TIMEOUT_SECONDS,
            "maxOutputBytes": policy.max_output_bytes,
            "stdoutTruncated": output.stdout_truncated,
            "stderrTruncated": output.stderr_truncated,
            "allowlist": kubernetes_policy_summary(&policy),
            "receivedArguments": arguments
        },
        "isError": false
    })
}

fn kubernetes_get_result(
    query: KubernetesGetQuery,
    policy: KubernetesResourcePolicy,
    kubectl_args: Vec<String>,
    output: KubectlRunOutput,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    let succeeded = kubectl_succeeded(&output);
    if !succeeded {
        return kubernetes_command_result(
            TOOL_GET_KUBERNETES_RESOURCE,
            format!(
                "kubectl get {}/{} in namespace {} {}",
                query.resource,
                query.name,
                query.namespace,
                kubectl_exit_summary(&output, DEFAULT_KUBECTL_TIMEOUT_SECONDS)
            ),
            query.source.clone(),
            query.namespace,
            query.resource,
            Some(query.name),
            kubectl_args,
            DEFAULT_KUBECTL_TIMEOUT_SECONDS,
            policy.max_output_bytes,
            Some(policy),
            output,
            credential_version,
            arguments,
        );
    }

    let parsed = match parse_kubectl_json_output(&output) {
        Ok(parsed) => parsed,
        Err(message) => {
            return kubernetes_json_error_result(
                TOOL_GET_KUBERNETES_RESOURCE,
                &message,
                query.source.clone(),
                query.namespace,
                query.resource,
                Some(query.name),
                kubectl_args,
                policy.max_output_bytes,
                Some(policy),
                output,
                credential_version,
                arguments,
            );
        }
    };
    let summary = summarize_kubernetes_resource(&query.resource, &parsed);
    let command = kubectl_command(&kubectl_args);

    json!({
        "content": [
            {
                "type": "text",
                "text": format!(
                    "{TOOL_GET_KUBERNETES_RESOURCE}: returned summary for {}/{} in namespace {}",
                    query.resource,
                    query.name,
                    query.namespace
                )
            }
        ],
        "structuredContent": {
            "status": "succeeded",
            "action": TOOL_GET_KUBERNETES_RESOURCE,
            "sourceName": query.source.source_name,
            "credentialVersion": credential_version,
            "namespace": query.namespace,
            "resource": query.resource,
            "name": query.name,
            "resourceSummary": summary,
            "command": command,
            "exitCode": output.exit_code,
            "timedOut": output.timed_out,
            "timeoutSeconds": DEFAULT_KUBECTL_TIMEOUT_SECONDS,
            "maxOutputBytes": policy.max_output_bytes,
            "stdoutTruncated": output.stdout_truncated,
            "stderrTruncated": output.stderr_truncated,
            "allowlist": kubernetes_policy_summary(&policy),
            "receivedArguments": arguments
        },
        "isError": false
    })
}

fn kubernetes_rollout_result(
    query: KubernetesRolloutQuery,
    policy: KubernetesResourcePolicy,
    kubectl_args: Vec<String>,
    output: KubectlRunOutput,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    let succeeded = kubectl_succeeded(&output);
    let status = kubectl_status(&output);
    let command = kubectl_command(&kubectl_args);
    let output_truncated = output.stdout_truncated;

    json!({
        "content": [
            {
                "type": "text",
                "text": format!(
                    "{TOOL_KUBERNETES_ROLLOUT_STATUS}: rollout {} for {}/{} in namespace {} {}",
                    query.action,
                    query.resource,
                    query.name,
                    query.namespace,
                    kubectl_exit_summary(&output, DEFAULT_KUBECTL_TIMEOUT_SECONDS)
                )
            }
        ],
        "structuredContent": {
            "status": status,
            "action": TOOL_KUBERNETES_ROLLOUT_STATUS,
            "sourceName": query.source.source_name,
            "credentialVersion": credential_version,
            "namespace": query.namespace,
            "resource": query.resource,
            "name": query.name,
            "actionType": query.action,
            "revision": query.revision,
            "command": command,
            "exitCode": output.exit_code,
            "timedOut": output.timed_out,
            "timeoutSeconds": DEFAULT_KUBECTL_TIMEOUT_SECONDS,
            "maxOutputBytes": policy.max_output_bytes,
            "output": output.stdout,
            "stderr": output.stderr,
            "outputTruncated": output_truncated,
            "stderrTruncated": output.stderr_truncated,
            "allowlist": kubernetes_policy_summary(&policy),
            "receivedArguments": arguments
        },
        "isError": !succeeded
    })
}

fn kubectl_run_result(
    args: Vec<String>,
    timeout_seconds: u64,
    max_output_bytes: usize,
    policy: Option<KubernetesResourcePolicy>,
    output: KubectlRunOutput,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    let succeeded = kubectl_succeeded(&output);
    let status = kubectl_status(&output);
    let exit_summary = kubectl_exit_summary(&output, timeout_seconds);
    let command = kubectl_command(&args);

    json!({
        "content": [
            {
                "type": "text",
                "text": format!("{TOOL_RUN_KUBECTL_READ}: kubectl {} {exit_summary}", args.join(" "))
            }
        ],
        "structuredContent": {
            "status": status,
            "action": TOOL_RUN_KUBECTL_READ,
            "command": command,
            "args": args,
            "exitCode": output.exit_code,
            "timedOut": output.timed_out,
            "timeoutSeconds": timeout_seconds,
            "maxOutputBytes": max_output_bytes,
            "credentialVersion": credential_version,
            "stdout": output.stdout,
            "stderr": output.stderr,
            "stdoutTruncated": output.stdout_truncated,
            "stderrTruncated": output.stderr_truncated,
            "allowlist": policy.as_ref().map(kubernetes_policy_summary),
            "receivedArguments": arguments
        },
        "isError": !succeeded
    })
}

fn pod_log_query_result(
    query: PodLogQuery,
    policy: KubernetesResourcePolicy,
    output: KubectlRunOutput,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    let succeeded = kubectl_succeeded(&output);
    let status = kubectl_status(&output);
    let line_count = output.stdout.lines().count();
    let command = kubectl_command(&query.kubectl_args);

    json!({
        "content": [
            {
                "type": "text",
                "text": format!(
                    "{TOOL_QUERY_POD_LOGS}: returned {line_count} log line(s) for pod {}/{}",
                    query.namespace,
                    query.pod_name
                )
            }
        ],
        "structuredContent": {
            "status": status,
            "action": TOOL_QUERY_POD_LOGS,
            "sourceName": query.source.source_name,
            "credentialVersion": credential_version,
            "namespace": query.namespace,
            "podName": query.pod_name,
            "container": query.container,
            "since": query.since,
            "tailLines": query.tail_lines,
            "previous": query.previous,
            "timestamps": query.timestamps,
            "lineCount": line_count,
            "command": command,
            "exitCode": output.exit_code,
            "timedOut": output.timed_out,
            "timeoutSeconds": query.timeout_seconds,
            "maxOutputBytes": query.max_output_bytes,
            "logs": output.stdout,
            "stderr": output.stderr,
            "logsTruncated": output.stdout_truncated,
            "stderrTruncated": output.stderr_truncated,
            "allowlist": kubernetes_policy_summary(&policy),
            "receivedArguments": arguments
        },
        "isError": !succeeded
    })
}

#[allow(clippy::too_many_arguments)]
fn kubernetes_command_result(
    action: &str,
    message: String,
    source: control_plane::SourceRef,
    namespace: String,
    resource: String,
    name: Option<String>,
    args: Vec<String>,
    timeout_seconds: u64,
    max_output_bytes: usize,
    policy: Option<KubernetesResourcePolicy>,
    output: KubectlRunOutput,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    let status = kubectl_status(&output);
    let command = kubectl_command(&args);

    json!({
        "content": [
            {
                "type": "text",
                "text": format!("{action}: {message}")
            }
        ],
        "structuredContent": {
            "status": status,
            "action": action,
            "message": message,
            "sourceName": source.source_name,
            "credentialVersion": credential_version,
            "namespace": namespace,
            "resource": resource,
            "name": name,
            "command": command,
            "exitCode": output.exit_code,
            "timedOut": output.timed_out,
            "timeoutSeconds": timeout_seconds,
            "maxOutputBytes": max_output_bytes,
            "stderr": output.stderr,
            "stdoutTruncated": output.stdout_truncated,
            "stderrTruncated": output.stderr_truncated,
            "allowlist": policy.as_ref().map(kubernetes_policy_summary),
            "receivedArguments": arguments
        },
        "isError": true
    })
}

#[allow(clippy::too_many_arguments)]
fn kubernetes_json_error_result(
    action: &str,
    message: &str,
    source: control_plane::SourceRef,
    namespace: String,
    resource: String,
    name: Option<String>,
    args: Vec<String>,
    max_output_bytes: usize,
    policy: Option<KubernetesResourcePolicy>,
    output: KubectlRunOutput,
    credential_version: Option<i64>,
    arguments: &Value,
) -> Value {
    kubernetes_command_result(
        action,
        format!("failed to parse kubectl JSON output: {message}"),
        source,
        namespace,
        resource,
        name,
        args,
        DEFAULT_KUBECTL_TIMEOUT_SECONDS,
        max_output_bytes,
        policy,
        output,
        credential_version,
        arguments,
    )
}

fn kubectl_succeeded(output: &KubectlRunOutput) -> bool {
    !output.timed_out && output.exit_code == Some(0)
}

fn kubectl_status(output: &KubectlRunOutput) -> &'static str {
    if output.timed_out {
        "timeout"
    } else if kubectl_succeeded(output) {
        "succeeded"
    } else {
        "failed"
    }
}

fn kubectl_exit_summary(output: &KubectlRunOutput, timeout_seconds: u64) -> String {
    match (output.timed_out, output.exit_code) {
        (true, _) => format!("timed out after {timeout_seconds}s"),
        (false, Some(exit_code)) => format!("exited with code {exit_code}"),
        (false, None) => "exited without an exit code".to_string(),
    }
}

fn kubectl_command(args: &[String]) -> Vec<String> {
    let mut command = Vec::with_capacity(args.len() + 1);
    command.push("kubectl".to_string());
    command.extend(args.iter().cloned());
    command
}

fn parse_kubectl_json_output(output: &KubectlRunOutput) -> Result<Value, String> {
    if output.stdout_truncated {
        return Err("output exceeded allowlist max_output_bytes".to_string());
    }
    serde_json::from_str::<Value>(&output.stdout).map_err(|error| error.to_string())
}

fn kubernetes_policy_summary(policy: &KubernetesResourcePolicy) -> Value {
    json!({
        "sourceName": policy.source_name,
        "namespace": policy.namespace,
        "resource": policy.resource,
        "actions": policy.actions,
        "maxItems": policy.max_items,
        "maxOutputBytes": policy.max_output_bytes,
        "maxTailLines": policy.max_tail_lines
    })
}

fn summarize_kubernetes_resource(resource: &str, value: &Value) -> Value {
    let mut summary = serde_json::Map::new();
    insert_pointer_value(&mut summary, "apiVersion", value, "/apiVersion");
    insert_pointer_value(&mut summary, "kind", value, "/kind");
    summary.insert("metadata".to_string(), summarize_kubernetes_metadata(value));

    match resource {
        "pods" => {
            summary.insert("status".to_string(), summarize_pod_status(value));
        }
        "deployments" | "statefulsets" | "daemonsets" => {
            summary.insert("spec".to_string(), summarize_workload_spec(value));
            summary.insert("status".to_string(), summarize_workload_status(value));
        }
        "services" => {
            summary.insert("spec".to_string(), summarize_service_spec(value));
        }
        "ingresses" => {
            summary.insert("spec".to_string(), summarize_ingress_spec(value));
            summary.insert("status".to_string(), summarize_ingress_status(value));
        }
        "jobs" => {
            summary.insert("spec".to_string(), summarize_job_spec(value));
            summary.insert("status".to_string(), summarize_job_status(value));
        }
        "persistentvolumeclaims" => {
            summary.insert("spec".to_string(), summarize_pvc_spec(value));
            summary.insert("status".to_string(), summarize_pvc_status(value));
        }
        "events" => {
            summary.insert("event".to_string(), summarize_event(value));
        }
        _ => {
            summary.insert("status".to_string(), summarize_generic_status(value));
        }
    }

    Value::Object(summary)
}

fn summarize_kubernetes_metadata(value: &Value) -> Value {
    let mut metadata = serde_json::Map::new();
    insert_pointer_value(&mut metadata, "name", value, "/metadata/name");
    insert_pointer_value(&mut metadata, "namespace", value, "/metadata/namespace");
    insert_pointer_value(
        &mut metadata,
        "creationTimestamp",
        value,
        "/metadata/creationTimestamp",
    );
    insert_pointer_value(&mut metadata, "generation", value, "/metadata/generation");
    if let Some(labels) = value.pointer("/metadata/labels") {
        metadata.insert("labels".to_string(), summarize_string_map(labels, 32, 128));
    }
    if let Some(owner_references) = value.pointer("/metadata/ownerReferences") {
        metadata.insert(
            "ownerReferences".to_string(),
            summarize_owner_references(owner_references),
        );
    }

    Value::Object(metadata)
}

fn summarize_pod_status(value: &Value) -> Value {
    let mut status = serde_json::Map::new();
    for (key, pointer) in [
        ("phase", "/status/phase"),
        ("podIP", "/status/podIP"),
        ("hostIP", "/status/hostIP"),
        ("startTime", "/status/startTime"),
        ("qosClass", "/status/qosClass"),
        ("reason", "/status/reason"),
        ("message", "/status/message"),
    ] {
        insert_pointer_value(&mut status, key, value, pointer);
    }
    if let Some(conditions) = value.pointer("/status/conditions") {
        status.insert("conditions".to_string(), summarize_conditions(conditions));
    }
    if let Some(container_statuses) = value.pointer("/status/containerStatuses") {
        status.insert(
            "containers".to_string(),
            summarize_container_statuses(container_statuses),
        );
    }

    Value::Object(status)
}

fn summarize_workload_spec(value: &Value) -> Value {
    let mut spec = serde_json::Map::new();
    for (key, pointer) in [
        ("replicas", "/spec/replicas"),
        ("strategyType", "/spec/strategy/type"),
        ("serviceName", "/spec/serviceName"),
        ("updateStrategyType", "/spec/updateStrategy/type"),
        ("minReadySeconds", "/spec/minReadySeconds"),
    ] {
        insert_pointer_value(&mut spec, key, value, pointer);
    }
    if let Some(selector) = value.pointer("/spec/selector/matchLabels") {
        spec.insert(
            "selector".to_string(),
            summarize_string_map(selector, 32, 128),
        );
    }

    Value::Object(spec)
}

fn summarize_workload_status(value: &Value) -> Value {
    let mut status = serde_json::Map::new();
    for (key, pointer) in [
        ("observedGeneration", "/status/observedGeneration"),
        ("replicas", "/status/replicas"),
        ("readyReplicas", "/status/readyReplicas"),
        ("availableReplicas", "/status/availableReplicas"),
        ("updatedReplicas", "/status/updatedReplicas"),
        ("currentReplicas", "/status/currentReplicas"),
        ("currentRevision", "/status/currentRevision"),
        ("updateRevision", "/status/updateRevision"),
        ("desiredNumberScheduled", "/status/desiredNumberScheduled"),
        ("currentNumberScheduled", "/status/currentNumberScheduled"),
        ("numberReady", "/status/numberReady"),
        ("numberAvailable", "/status/numberAvailable"),
        ("numberUnavailable", "/status/numberUnavailable"),
    ] {
        insert_pointer_value(&mut status, key, value, pointer);
    }
    if let Some(conditions) = value.pointer("/status/conditions") {
        status.insert("conditions".to_string(), summarize_conditions(conditions));
    }

    Value::Object(status)
}

fn summarize_service_spec(value: &Value) -> Value {
    let mut spec = serde_json::Map::new();
    for (key, pointer) in [
        ("type", "/spec/type"),
        ("clusterIP", "/spec/clusterIP"),
        ("externalName", "/spec/externalName"),
        ("ipFamilyPolicy", "/spec/ipFamilyPolicy"),
    ] {
        insert_pointer_value(&mut spec, key, value, pointer);
    }
    if let Some(selector) = value.pointer("/spec/selector") {
        spec.insert(
            "selector".to_string(),
            summarize_string_map(selector, 32, 128),
        );
    }
    if let Some(ports) = value.pointer("/spec/ports").and_then(Value::as_array) {
        spec.insert(
            "ports".to_string(),
            Value::Array(
                ports
                    .iter()
                    .take(32)
                    .map(summarize_service_port)
                    .collect::<Vec<_>>(),
            ),
        );
    }

    Value::Object(spec)
}

fn summarize_service_port(port: &Value) -> Value {
    let mut summary = serde_json::Map::new();
    for key in ["name", "protocol", "port", "targetPort", "nodePort"] {
        if let Some(value) = port.get(key).filter(|value| !value.is_null()) {
            summary.insert(key.to_string(), value.clone());
        }
    }

    Value::Object(summary)
}

fn summarize_ingress_spec(value: &Value) -> Value {
    let mut spec = serde_json::Map::new();
    insert_pointer_value(
        &mut spec,
        "ingressClassName",
        value,
        "/spec/ingressClassName",
    );
    if let Some(rules) = value.pointer("/spec/rules").and_then(Value::as_array) {
        spec.insert(
            "hosts".to_string(),
            Value::Array(
                rules
                    .iter()
                    .filter_map(|rule| rule.get("host").and_then(Value::as_str))
                    .take(64)
                    .map(|host| Value::String(truncate_string(host, 255)))
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if let Some(tls) = value.pointer("/spec/tls").and_then(Value::as_array) {
        spec.insert("tlsCount".to_string(), json!(tls.len()));
    }

    Value::Object(spec)
}

fn summarize_ingress_status(value: &Value) -> Value {
    let mut status = serde_json::Map::new();
    if let Some(ingress) = value.pointer("/status/loadBalancer/ingress") {
        status.insert("loadBalancerIngress".to_string(), ingress.clone());
    }

    Value::Object(status)
}

fn summarize_job_spec(value: &Value) -> Value {
    let mut spec = serde_json::Map::new();
    for (key, pointer) in [
        ("parallelism", "/spec/parallelism"),
        ("completions", "/spec/completions"),
        ("backoffLimit", "/spec/backoffLimit"),
        ("completionMode", "/spec/completionMode"),
    ] {
        insert_pointer_value(&mut spec, key, value, pointer);
    }

    Value::Object(spec)
}

fn summarize_job_status(value: &Value) -> Value {
    let mut status = serde_json::Map::new();
    for (key, pointer) in [
        ("active", "/status/active"),
        ("succeeded", "/status/succeeded"),
        ("failed", "/status/failed"),
        ("startTime", "/status/startTime"),
        ("completionTime", "/status/completionTime"),
    ] {
        insert_pointer_value(&mut status, key, value, pointer);
    }
    if let Some(conditions) = value.pointer("/status/conditions") {
        status.insert("conditions".to_string(), summarize_conditions(conditions));
    }

    Value::Object(status)
}

fn summarize_pvc_spec(value: &Value) -> Value {
    let mut spec = serde_json::Map::new();
    for (key, pointer) in [
        ("storageClassName", "/spec/storageClassName"),
        ("volumeName", "/spec/volumeName"),
        ("volumeMode", "/spec/volumeMode"),
        ("storage", "/spec/resources/requests/storage"),
    ] {
        insert_pointer_value(&mut spec, key, value, pointer);
    }
    if let Some(access_modes) = value.pointer("/spec/accessModes") {
        spec.insert("accessModes".to_string(), access_modes.clone());
    }

    Value::Object(spec)
}

fn summarize_pvc_status(value: &Value) -> Value {
    let mut status = serde_json::Map::new();
    for (key, pointer) in [
        ("phase", "/status/phase"),
        ("capacityStorage", "/status/capacity/storage"),
    ] {
        insert_pointer_value(&mut status, key, value, pointer);
    }
    if let Some(access_modes) = value.pointer("/status/accessModes") {
        status.insert("accessModes".to_string(), access_modes.clone());
    }

    Value::Object(status)
}

fn summarize_event(value: &Value) -> Value {
    let mut event = serde_json::Map::new();
    for (key, pointer) in [
        ("type", "/type"),
        ("reason", "/reason"),
        ("message", "/message"),
        ("count", "/count"),
        ("firstTimestamp", "/firstTimestamp"),
        ("lastTimestamp", "/lastTimestamp"),
        ("eventTime", "/eventTime"),
        ("reportingComponent", "/reportingComponent"),
        ("reportingInstance", "/reportingInstance"),
    ] {
        insert_pointer_value(&mut event, key, value, pointer);
    }
    if let Some(involved) = value.get("involvedObject") {
        let mut object = serde_json::Map::new();
        for key in ["kind", "namespace", "name", "apiVersion", "fieldPath"] {
            if let Some(value) = involved.get(key).filter(|value| !value.is_null()) {
                object.insert(key.to_string(), truncate_json_string(value, 255));
            }
        }
        event.insert("involvedObject".to_string(), Value::Object(object));
    }

    Value::Object(event)
}

fn summarize_generic_status(value: &Value) -> Value {
    let mut status = serde_json::Map::new();
    if let Some(conditions) = value.pointer("/status/conditions") {
        status.insert("conditions".to_string(), summarize_conditions(conditions));
    }
    insert_pointer_value(&mut status, "phase", value, "/status/phase");

    Value::Object(status)
}

fn summarize_conditions(conditions: &Value) -> Value {
    let Some(conditions) = conditions.as_array() else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        conditions
            .iter()
            .take(32)
            .map(|condition| {
                let mut summary = serde_json::Map::new();
                for key in [
                    "type",
                    "status",
                    "reason",
                    "message",
                    "lastTransitionTime",
                    "lastUpdateTime",
                ] {
                    if let Some(value) = condition.get(key).filter(|value| !value.is_null()) {
                        summary.insert(key.to_string(), truncate_json_string(value, 512));
                    }
                }
                Value::Object(summary)
            })
            .collect::<Vec<_>>(),
    )
}

fn summarize_container_statuses(container_statuses: &Value) -> Value {
    let Some(container_statuses) = container_statuses.as_array() else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        container_statuses
            .iter()
            .take(64)
            .map(|container| {
                let mut summary = serde_json::Map::new();
                for key in ["name", "ready", "restartCount", "image"] {
                    if let Some(value) = container.get(key).filter(|value| !value.is_null()) {
                        summary.insert(key.to_string(), truncate_json_string(value, 255));
                    }
                }
                if let Some(state) = container.get("state") {
                    summary.insert("state".to_string(), summarize_container_state(state));
                }
                Value::Object(summary)
            })
            .collect::<Vec<_>>(),
    )
}

fn summarize_container_state(state: &Value) -> Value {
    let mut summary = serde_json::Map::new();
    for state_name in ["waiting", "running", "terminated"] {
        let Some(state_value) = state.get(state_name) else {
            continue;
        };
        let mut state_summary = serde_json::Map::new();
        for key in [
            "reason",
            "message",
            "startedAt",
            "finishedAt",
            "exitCode",
            "signal",
        ] {
            if let Some(value) = state_value.get(key).filter(|value| !value.is_null()) {
                state_summary.insert(key.to_string(), truncate_json_string(value, 512));
            }
        }
        summary.insert(state_name.to_string(), Value::Object(state_summary));
    }

    Value::Object(summary)
}

fn summarize_string_map(value: &Value, max_items: usize, max_value_chars: usize) -> Value {
    let Some(object) = value.as_object() else {
        return Value::Object(serde_json::Map::new());
    };
    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort();
    let mut summary = serde_json::Map::new();
    for key in keys.into_iter().take(max_items) {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            summary.insert(
                key.chars().take(128).collect::<String>(),
                Value::String(truncate_string(value, max_value_chars)),
            );
        }
    }

    Value::Object(summary)
}

fn summarize_owner_references(owner_references: &Value) -> Value {
    let Some(owner_references) = owner_references.as_array() else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        owner_references
            .iter()
            .take(16)
            .map(|owner| {
                let mut summary = serde_json::Map::new();
                for key in ["apiVersion", "kind", "name", "controller"] {
                    if let Some(value) = owner.get(key).filter(|value| !value.is_null()) {
                        summary.insert(key.to_string(), truncate_json_string(value, 255));
                    }
                }
                Value::Object(summary)
            })
            .collect::<Vec<_>>(),
    )
}

fn insert_pointer_value(
    map: &mut serde_json::Map<String, Value>,
    key: &str,
    value: &Value,
    pointer: &str,
) {
    let Some(value) = value.pointer(pointer).filter(|value| !value.is_null()) else {
        return;
    };
    map.insert(key.to_string(), truncate_json_string(value, 512));
}

fn truncate_json_string(value: &Value, max_chars: usize) -> Value {
    match value {
        Value::String(value) => Value::String(truncate_string(value, max_chars)),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .take(64)
                .map(|value| truncate_json_string(value, max_chars))
                .collect(),
        ),
        Value::Object(object) => {
            let mut summary = serde_json::Map::new();
            for (key, value) in object.iter().take(64) {
                summary.insert(
                    key.chars().take(128).collect::<String>(),
                    truncate_json_string(value, max_chars),
                );
            }
            Value::Object(summary)
        }
        _ => value.clone(),
    }
}

fn truncate_string(value: &str, max_chars: usize) -> String {
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        truncated.push_str("...");
    }
    truncated
}

fn tool_error_result(action: &str, status: &str, message: &str, arguments: &Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": format!("{action}: {message}")
            }
        ],
        "structuredContent": {
            "status": status,
            "action": action,
            "message": message,
            "receivedArguments": arguments
        },
        "isError": true
    })
}

fn tool_error_result_with_authorization(
    action: &str,
    status: &str,
    message: &str,
    arguments: &Value,
    scope: &control_plane::ToolAuthorizationScope,
    decision: &control_plane::AccessDecision,
) -> Value {
    with_authorization_summary(
        tool_error_result(action, status, message, arguments),
        scope,
        decision,
    )
}

fn with_authorization_summary(
    mut result: Value,
    scope: &control_plane::ToolAuthorizationScope,
    decision: &control_plane::AccessDecision,
) -> Value {
    if let Some(structured) = result
        .get_mut("structuredContent")
        .and_then(Value::as_object_mut)
    {
        structured.insert(
            "authorization".to_string(),
            control_plane::access_decision_summary(scope, decision),
        );
        structured
            .entry("sourceName".to_string())
            .or_insert_with(|| Value::String(scope.source.source_name.clone()));
    }

    result
}

fn explain_gate_error_result(
    action: &str,
    message: &str,
    explain_gate: ExplainGateReport,
    arguments: &Value,
) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": format!(
                    "{action}: {message}: estimated rows {} exceeds max_estimated_rows {}",
                    explain_gate.estimated_rows,
                    explain_gate.max_estimated_rows
                )
            }
        ],
        "structuredContent": {
            "status": "explain_gate_rejected",
            "action": action,
            "message": message,
            "explainGate": explain_gate,
            "receivedArguments": arguments
        },
        "isError": true
    })
}

fn tool_argument_error(action: &str, message: &str, arguments: &Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": format!("{action}: {message}")
            }
        ],
        "structuredContent": {
            "status": "invalid_arguments",
            "action": action,
            "message": message,
            "receivedArguments": arguments
        },
        "isError": true
    })
}

fn is_valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn quote_identifier(value: &str) -> String {
    format!("`{value}`")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Execute;

    async fn test_store() -> FileStore {
        let path = std::env::temp_dir().join(format!(
            "action-gateway-actions-test-{}.json",
            uuid::Uuid::new_v4().simple()
        ));
        FileStore::load(path).await.expect("test store should load")
    }

    fn test_redis() -> RedisClient {
        RedisClient::open("redis://127.0.0.1:6379/").expect("test redis client should be created")
    }

    fn kubectl_args(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_string()).collect()
    }

    fn kubernetes_policy(resource: &str) -> KubernetesResourcePolicy {
        KubernetesResourcePolicy {
            source_name: "default".to_string(),
            namespace: "default".to_string(),
            resource: resource.to_string(),
            actions: vec![
                "list".to_string(),
                "get".to_string(),
                "describe".to_string(),
                "logs".to_string(),
            ],
            max_items: 100,
            max_output_bytes: 65_536,
            max_tail_lines: 1000,
        }
    }

    #[test]
    fn lists_gateway_tools() {
        let tools = list_tools();
        let tool_names = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(tool_names[0], TOOL_QUERY_TABLE_DATA);
        assert!(tool_names.contains(&TOOL_QUERY_REDIS_KEY));
        assert!(tool_names.contains(&TOOL_LIST_KUBERNETES_RESOURCES));
        assert!(tool_names.contains(&TOOL_GET_KUBERNETES_RESOURCE));
        assert!(tool_names.contains(&TOOL_KUBERNETES_ROLLOUT_STATUS));
        assert!(tool_names.contains(&TOOL_QUERY_POD_LOGS));
        assert!(tool_names.contains(&TOOL_QUERY_APP_LOGS));
        assert!(tool_names.contains(&audit::TOOL_QUERY_APPROVAL_AUDIT_EVENTS));
        assert_eq!(
            tool_names.contains(&TOOL_RUN_KUBECTL_READ),
            raw_kubectl_enabled()
        );
    }

    #[test]
    fn builds_pod_log_kubectl_args() {
        let query = parse_pod_log_query(&json!({
            "namespace": "default",
            "pod_name": "api-0",
            "container": "api",
            "since": "15m",
            "tail_lines": 10,
            "previous": true,
            "timestamps": true,
            "timeout_seconds": 12,
            "max_output_bytes": 4096
        }))
        .expect("pod log query should parse");

        assert_eq!(query.namespace, "default");
        assert_eq!(query.pod_name, "api-0");
        assert_eq!(query.container.as_deref(), Some("api"));
        assert_eq!(
            query.kubectl_args,
            kubectl_args(&[
                "logs",
                "api-0",
                "-n",
                "default",
                "--tail",
                "10",
                "-c",
                "api",
                "--since",
                "15m",
                "--previous",
                "--timestamps"
            ])
        );
    }

    #[test]
    fn parses_app_log_query_defaults_and_filters() {
        let query = parse_app_log_query(&json!({
            "app_name": "billing-api",
            "environment": "prod",
            "trace_id": "trc_paid_summary_001",
            "keyword": "summary",
            "limit": 10
        }))
        .expect("app log query should parse");

        assert_eq!(query.app_name, "billing-api");
        assert_eq!(query.environment.as_deref(), Some("prod"));
        assert_eq!(query.trace_id.as_deref(), Some("trc_paid_summary_001"));
        assert_eq!(query.keyword.as_deref(), Some("summary"));
        assert_eq!(query.limit, 10);
        assert_eq!(query.index_key, "app_logs:index:app_env:billing-api:prod");

        let default_query = parse_app_log_query(&json!({
            "app_name": "billing-api"
        }))
        .expect("app log query should parse with defaults");
        assert_eq!(default_query.limit, DEFAULT_APP_LOG_LIMIT);
        assert_eq!(default_query.index_key, "app_logs:index:app:billing-api");
    }

    #[test]
    fn rejects_invalid_app_log_query_arguments() {
        assert_eq!(
            parse_app_log_query(&json!({})).unwrap_err(),
            "missing required argument: app_name"
        );
        assert_eq!(
            parse_app_log_query(&json!({
                "app_name": "billing:api"
            }))
            .unwrap_err(),
            "app_name may contain only ASCII letters, numbers, '.', '-', and '_'"
        );
        assert_eq!(
            parse_app_log_query(&json!({
                "app_name": "billing-api",
                "limit": 201
            }))
            .unwrap_err(),
            "limit must be between 1 and 200"
        );
        assert_eq!(
            parse_app_log_query(&json!({
                "app_name": "billing-api",
                "since": "yesterday"
            }))
            .unwrap_err(),
            "since must be a duration such as 15m or 1h"
        );
    }

    #[test]
    fn filters_and_summarizes_app_log_entries() {
        let query = parse_app_log_query(&json!({
            "app_name": "billing-api",
            "environment": "prod",
            "trace_id": "trc_paid_summary_001",
            "keyword": "12.00",
            "limit": 10
        }))
        .expect("app log query should parse");
        let entry = json!({
            "id": "log_1001",
            "timestamp": "2026-05-14T03:00:00Z",
            "app_name": "billing-api",
            "environment": "prod",
            "level": "ERROR",
            "trace_id": "trc_paid_summary_001",
            "message": "paid summary returned 12.00 for customer order page",
            "fields": {
                "endpoint": "/api/orders/paid-summary",
                "status_code": 200
            }
        });

        assert!(app_log_entry_matches(&query, &entry));
        let summary = summarize_app_log_entry(&entry).expect("entry should summarize");
        assert_eq!(summary["id"], "log_1001");
        assert_eq!(summary["traceId"], "trc_paid_summary_001");
        assert_eq!(summary["fields"]["endpoint"], "/api/orders/paid-summary");
        assert_eq!(summary.get("trace_id"), None);
    }

    #[test]
    fn formats_app_log_query_result() {
        let query = parse_app_log_query(&json!({
            "app_name": "billing-api",
            "limit": 1
        }))
        .expect("app log query should parse");
        let entry = summarize_app_log_entry(&json!({
            "id": "log_1001",
            "timestamp": "2026-05-14T03:00:00Z",
            "app_name": "billing-api",
            "environment": "prod",
            "level": "ERROR",
            "trace_id": "trc_paid_summary_001",
            "message": "paid summary returned 12.00 for customer order page",
            "fields": {
                "endpoint": "/api/orders/paid-summary"
            }
        }))
        .expect("entry should summarize");
        let result = app_log_query_result(
            control_plane::SourceRef::legacy_default(),
            None,
            query,
            AppLogRead {
                entries: vec![entry],
                scanned_count: 2,
                returned_count: 1,
                truncated: true,
            },
            &json!({"app_name": "billing-api", "limit": 1}),
        );

        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["status"], "succeeded");
        assert_eq!(result["structuredContent"]["returnedCount"], 1);
        assert_eq!(result["structuredContent"]["truncated"], true);
        assert_eq!(result["structuredContent"]["logs"][0]["id"], "log_1001");
    }

    #[tokio::test]
    async fn returns_not_allowed_when_raw_kubectl_is_disabled() {
        let store = test_store().await;
        let redis = test_redis();
        let request = json!({
            "params": {
                "name": TOOL_RUN_KUBECTL_READ,
                "arguments": {
                    "args": ["delete", "pod", "api-0"]
                }
            }
        });

        let result = call_tool(&store, &redis, &request)
            .await
            .expect("tool call should return a tool error result");

        assert_eq!(result["isError"], true);
        if raw_kubectl_enabled() {
            assert_eq!(result["structuredContent"]["status"], "invalid_arguments");
        } else {
            assert_eq!(result["structuredContent"]["status"], "not_allowed");
        }
    }

    #[tokio::test]
    async fn reports_missing_required_argument_as_tool_error() {
        let store = test_store().await;
        let redis = test_redis();
        let request = json!({
            "params": {
                "name": TOOL_QUERY_POD_LOGS,
                "arguments": {
                    "namespace": "default"
                }
            }
        });

        let result = call_tool(&store, &redis, &request)
            .await
            .expect("tool call should return a tool error result");

        assert_eq!(result["isError"], true);
        assert_eq!(result["structuredContent"]["status"], "invalid_arguments");
    }

    #[tokio::test]
    async fn rejects_unknown_tool() {
        let store = test_store().await;
        let redis = test_redis();
        let request = json!({
            "params": {
                "name": "missing.tool",
                "arguments": {}
            }
        });

        let error = call_tool(&store, &redis, &request)
            .await
            .expect_err("unknown tool should fail at protocol level");

        assert_eq!(error, (-32602, "unknown tool"));
    }

    #[test]
    fn parses_and_quotes_table_paths() {
        assert_eq!(
            parse_table_path("orders").unwrap(),
            TablePath {
                schema: None,
                table: "orders".to_string(),
                quoted: "`orders`".to_string()
            }
        );
        assert_eq!(
            parse_table_path("analytics.orders").unwrap(),
            TablePath {
                schema: Some("analytics".to_string()),
                table: "orders".to_string(),
                quoted: "`analytics`.`orders`".to_string()
            }
        );
        assert!(parse_table_path("orders;drop").is_err());
    }

    #[test]
    fn parses_order_by_argument() {
        let allowed_columns = vec!["id".to_string(), "created_at".to_string()];
        let actual_columns = allowed_columns.clone();
        let order_by = parse_order_by_argument(
            &json!({
                "order_by": [
                    {"column": "created_at", "direction": "DESC"},
                    {"column": "id"}
                ]
            }),
            &allowed_columns,
            &actual_columns,
        )
        .unwrap();

        assert_eq!(
            order_by,
            vec![
                OrderBy {
                    column: "created_at".to_string(),
                    direction: SortDirection::Desc,
                },
                OrderBy {
                    column: "id".to_string(),
                    direction: SortDirection::Asc,
                }
            ]
        );
    }

    #[test]
    fn rejects_invalid_order_by_argument() {
        let allowed_columns = vec!["id".to_string()];
        let actual_columns = vec!["id".to_string(), "created_at".to_string()];

        assert_eq!(
            parse_order_by_argument(
                &json!({"order_by": [{"column": "created_at"}]}),
                &allowed_columns,
                &actual_columns,
            )
            .unwrap_err(),
            "order_by column is not allowlisted: created_at"
        );
        assert_eq!(
            parse_order_by_argument(
                &json!({"order_by": [{"column": "id", "direction": "sideways"}]}),
                &allowed_columns,
                &actual_columns,
            )
            .unwrap_err(),
            "order_by direction must be asc or desc"
        );
        assert_eq!(
            parse_order_by_argument(
                &json!({"order_by": [{"column": "id"}, {"column": "id"}]}),
                &allowed_columns,
                &actual_columns,
            )
            .unwrap_err(),
            "duplicate order_by column: id"
        );
    }

    #[test]
    fn builds_bound_table_query() {
        let table_path = parse_table_path("orders").unwrap();
        let columns = vec!["id".to_string(), "status".to_string()];
        let filters = vec![("status".to_string(), json!("paid"))];
        let mut builder = QueryBuilder::<MySql>::new("");

        append_table_query(&mut builder, &table_path, &columns, &filters, &[], 25).unwrap();

        assert_eq!(
            builder.build().sql(),
            "SELECT JSON_OBJECT('id', `id`, 'status', `status`) AS row_json FROM `orders` WHERE `status` <=> ? LIMIT ?"
        );
    }

    #[test]
    fn builds_bound_table_query_with_order_by() {
        let table_path = parse_table_path("orders").unwrap();
        let columns = vec!["id".to_string(), "status".to_string()];
        let filters = vec![("status".to_string(), json!("paid"))];
        let order_by = vec![
            OrderBy {
                column: "created_at".to_string(),
                direction: SortDirection::Desc,
            },
            OrderBy {
                column: "id".to_string(),
                direction: SortDirection::Asc,
            },
        ];
        let mut builder = QueryBuilder::<MySql>::new("");

        append_table_query(&mut builder, &table_path, &columns, &filters, &order_by, 25).unwrap();

        assert_eq!(
            builder.build().sql(),
            "SELECT JSON_OBJECT('id', `id`, 'status', `status`) AS row_json FROM `orders` WHERE `status` <=> ? ORDER BY `created_at` DESC, `id` ASC LIMIT ?"
        );
    }

    #[test]
    fn builds_bound_explain_query() {
        let table_path = parse_table_path("orders").unwrap();
        let columns = vec!["id".to_string(), "status".to_string()];
        let filters = vec![("status".to_string(), json!("paid"))];
        let mut builder = QueryBuilder::<MySql>::new("EXPLAIN ");

        append_table_query(&mut builder, &table_path, &columns, &filters, &[], 25).unwrap();

        assert_eq!(
            builder.build().sql(),
            "EXPLAIN SELECT JSON_OBJECT('id', `id`, 'status', `status`) AS row_json FROM `orders` WHERE `status` <=> ? LIMIT ?"
        );
    }

    #[test]
    fn includes_explain_gate_in_success_result() {
        let explain_gate = ExplainGateReport {
            max_estimated_rows: 1000,
            estimated_rows: 42,
            passed: true,
            plan: vec![ExplainPlanStep {
                select_type: Some("SIMPLE".to_string()),
                table: Some("orders".to_string()),
                access_type: Some("ref".to_string()),
                possible_keys: Some("idx_status".to_string()),
                key: Some("idx_status".to_string()),
                estimated_rows: 42,
                extra: Some("Using where".to_string()),
            }],
        };

        let result = table_query_result(
            &control_plane::SourceRef::legacy_default(),
            "orders",
            vec!["id".to_string()],
            vec![("status".to_string(), json!("paid"))],
            vec![OrderBy {
                column: "created_at".to_string(),
                direction: SortDirection::Desc,
            }],
            25,
            vec![json!({"id": 1})],
            explain_gate,
            MaskingReport {
                applied: false,
                masked_columns: Vec::new(),
            },
            None,
            &json!({"table_name": "orders"}),
        );

        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["explainGate"]["passed"], true);
        assert_eq!(
            result["structuredContent"]["explainGate"]["estimatedRows"],
            42
        );
        assert_eq!(
            result["structuredContent"]["explainGate"]["plan"][0]["accessType"],
            "ref"
        );
        assert_eq!(
            result["structuredContent"]["orderBy"],
            json!([{"column": "created_at", "direction": "desc"}])
        );
    }

    #[test]
    fn parses_and_applies_field_mask_rules() {
        let allowed_columns = vec![
            "id".to_string(),
            "customer_email".to_string(),
            "customer_phone".to_string(),
            "customer_name".to_string(),
        ];
        let actual_columns = allowed_columns.clone();
        let rules = parse_mask_rules(
            json!({
                "customer_email": "email",
                "customer_phone": {
                    "type": "partial",
                    "prefix": 3,
                    "suffix": 4
                },
                "customer_name": {
                    "type": "full",
                    "replacement": "[masked]"
                }
            }),
            &allowed_columns,
            &actual_columns,
        )
        .expect("mask rules should parse");

        let (rows, masking) = mask_rows(
            vec![json!({
                "id": 1,
                "customer_email": "alice@example.com",
                "customer_phone": "13800138000",
                "customer_name": "Alice"
            })],
            &allowed_columns,
            &rules,
        );

        assert!(masking.applied);
        assert_eq!(
            masking.masked_columns,
            vec![
                "customer_email".to_string(),
                "customer_name".to_string(),
                "customer_phone".to_string()
            ]
        );
        assert_eq!(rows[0]["customer_email"], "a****@example.com");
        assert_eq!(rows[0]["customer_phone"], "138****8000");
        assert_eq!(rows[0]["customer_name"], "[masked]");
        assert_eq!(rows[0]["id"], 1);
    }

    #[test]
    fn rejects_mask_rules_for_non_allowlisted_columns() {
        let error = parse_mask_rules(
            json!({
                "password_hash": "full"
            }),
            &["id".to_string()],
            &["id".to_string(), "password_hash".to_string()],
        )
        .expect_err("mask rule should be rejected");

        assert_eq!(error, "mask rule column is not allowlisted: password_hash");
    }

    #[test]
    fn validates_raw_kubectl_diagnostic_args() {
        assert!(
            validate_kubectl_read_args(&kubectl_args(&[
                "get", "pods", "-n", "default", "-o", "wide"
            ]))
            .is_ok()
        );
        assert!(validate_kubectl_read_args(&kubectl_args(&["config", "current-context"])).is_ok());
        assert!(
            validate_kubectl_read_args(&kubectl_args(&["rollout", "status", "deployment/api"]))
                .is_ok()
        );

        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&["delete", "pod", "api-0"])).unwrap_err(),
            "kubectl command is not allowed for raw diagnostic execution: delete"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&["get", "pods", "-A"])).unwrap_err(),
            "kubectl flag is not allowed: -A"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&[
                "get", "pods", "-n", "default", "-o", "json"
            ]))
            .unwrap_err(),
            "kubectl output format is not allowed: json"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&[
                "get",
                "pods",
                "-n",
                "default",
                "-o",
                "jsonpath={.items[*].metadata.name}"
            ]))
            .unwrap_err(),
            "kubectl output format is not allowed: jsonpath={.items[*].metadata.name}"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&["get", "pods", "--watch=false"]))
                .unwrap_err(),
            "kubectl flag is not allowed: --watch=false"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&["logs", "pod/api-0", "--follow=true"]))
                .unwrap_err(),
            "kubectl command is not allowed for raw diagnostic execution: logs"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&[
                "get",
                "pods",
                "--kubeconfig=/tmp/kubeconfig"
            ]))
            .unwrap_err(),
            "kubectl flag is not allowed: --kubeconfig=/tmp/kubeconfig"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&["config", "view"])).unwrap_err(),
            "kubectl config subcommand is not allowed: view"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&["rollout", "restart", "deployment/api"]))
                .unwrap_err(),
            "kubectl rollout subcommand is not allowed: restart"
        );
        assert_eq!(
            validate_kubectl_read_args(&kubectl_args(&["cluster-info", "dump"])).unwrap_err(),
            "kubectl command is not allowed for raw diagnostic execution: cluster-info"
        );
    }

    #[test]
    fn extracts_raw_kubectl_policy_subjects() {
        assert_eq!(
            raw_kubectl_policy_subject(&kubectl_args(&["get", "pods", "-n", "default"])).unwrap(),
            RawKubectlPolicySubject::Resource {
                namespace: "default".to_string(),
                resource: "pods".to_string(),
                name: None,
                action: "list".to_string()
            }
        );
        assert_eq!(
            raw_kubectl_policy_subject(&kubectl_args(&[
                "rollout",
                "status",
                "deployment/api",
                "-n",
                "default"
            ]))
            .unwrap(),
            RawKubectlPolicySubject::Resource {
                namespace: "default".to_string(),
                resource: "deployments".to_string(),
                name: Some("api".to_string()),
                action: "rollout_status".to_string()
            }
        );
        assert_eq!(
            raw_kubectl_policy_subject(&kubectl_args(&["get", "secrets", "-n", "default"]))
                .unwrap(),
            RawKubectlPolicySubject::Resource {
                namespace: "default".to_string(),
                resource: "secrets".to_string(),
                name: None,
                action: "list".to_string()
            }
        );
    }

    #[test]
    fn rejects_invalid_pod_log_query_arguments() {
        assert_eq!(
            parse_pod_log_query(&json!({
                "namespace": "Default",
                "pod_name": "api-0"
            }))
            .unwrap_err(),
            "namespace must start with a lowercase alphanumeric character"
        );
        assert_eq!(
            parse_pod_log_query(&json!({
                "namespace": "default",
                "pod_name": "api-0",
                "since": "yesterday"
            }))
            .unwrap_err(),
            "since must be a duration such as 15m or 1h"
        );
    }

    #[test]
    fn formats_kubectl_run_result() {
        let result = kubectl_run_result(
            kubectl_args(&["get", "pods", "-n", "default"]),
            10,
            4096,
            None,
            KubectlRunOutput {
                exit_code: Some(0),
                timed_out: false,
                stdout: "NAME READY\napi-0 1/1\n".to_string(),
                stderr: String::new(),
                stdout_truncated: false,
                stderr_truncated: false,
            },
            None,
            &json!({"args": ["get", "pods", "-n", "default"]}),
        );

        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["action"], TOOL_RUN_KUBECTL_READ);
        assert_eq!(result["structuredContent"]["status"], "succeeded");
        assert_eq!(result["structuredContent"]["command"][0], "kubectl");
        assert_eq!(
            result["structuredContent"]["stdout"],
            "NAME READY\napi-0 1/1\n"
        );
    }

    #[test]
    fn formats_pod_log_query_result() {
        let query = parse_pod_log_query(&json!({
            "namespace": "default",
            "pod_name": "api-0",
            "tail_lines": 2
        }))
        .expect("pod log query should parse");
        let result = pod_log_query_result(
            query,
            kubernetes_policy("pods"),
            KubectlRunOutput {
                exit_code: Some(0),
                timed_out: false,
                stdout: "first\nsecond\n".to_string(),
                stderr: String::new(),
                stdout_truncated: false,
                stderr_truncated: false,
            },
            None,
            &json!({"namespace": "default", "pod_name": "api-0", "tail_lines": 2}),
        );

        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["action"], TOOL_QUERY_POD_LOGS);
        assert_eq!(result["structuredContent"]["status"], "succeeded");
        assert_eq!(result["structuredContent"]["lineCount"], 2);
        assert_eq!(result["structuredContent"]["logs"], "first\nsecond\n");
        assert_eq!(result["structuredContent"]["command"][0], "kubectl");
    }

    #[test]
    fn matches_redis_key_allowlist_regex_against_entire_key() {
        let regex = Regex::new("app_logs:entry:log_[0-9]+").unwrap();

        assert!(regex_matches_entire_key(&regex, "app_logs:entry:log_1001"));
        assert!(!regex_matches_entire_key(
            &regex,
            "xapp_logs:entry:log_1001"
        ));
        assert!(!regex_matches_entire_key(
            &regex,
            "app_logs:entry:log_1001:x"
        ));
    }

    #[test]
    fn parses_redis_query_limit_against_allowlist_max() {
        assert_eq!(parse_redis_query_limit(&json!({}), 50).unwrap(), 50);
        assert_eq!(
            parse_redis_query_limit(&json!({"limit": 10}), 50).unwrap(),
            10
        );
        assert_eq!(
            parse_redis_query_limit(&json!({"limit": 51}), 50).unwrap_err(),
            "limit must be less than or equal to 50"
        );
    }

    #[test]
    fn formats_redis_key_query_result() {
        let result = redis_key_query_result(
            &control_plane::SourceRef::legacy_default(),
            "demo:user:1",
            AllowedRedisKey {
                pattern: "demo:[A-Za-z0-9_.:-]+".to_string(),
                max_value_bytes: 1024,
                max_members: 50,
            },
            10,
            RedisKeyRead {
                key_type: "string".to_string(),
                ttl_seconds: -1,
                data: json!({
                    "exists": true,
                    "valueLength": 5,
                    "value": "hello"
                }),
            },
            None,
            &json!({"key": "demo:user:1"}),
        );

        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["action"], TOOL_QUERY_REDIS_KEY);
        assert_eq!(result["structuredContent"]["key"], "demo:user:1");
        assert_eq!(
            result["structuredContent"]["allowlist"]["matchedPattern"],
            "demo:[A-Za-z0-9_.:-]+"
        );
        assert_eq!(result["structuredContent"]["value"], "hello");
    }

    #[test]
    fn formats_explain_gate_rejection() {
        let explain_gate = ExplainGateReport {
            max_estimated_rows: 1000,
            estimated_rows: 1001,
            passed: false,
            plan: vec![ExplainPlanStep {
                select_type: Some("SIMPLE".to_string()),
                table: Some("orders".to_string()),
                access_type: Some("ALL".to_string()),
                possible_keys: None,
                key: None,
                estimated_rows: 1001,
                extra: Some("Using where".to_string()),
            }],
        };

        let result = explain_gate_error_result(
            TOOL_QUERY_TABLE_DATA,
            "query rejected by EXPLAIN gate",
            explain_gate,
            &json!({"table_name": "orders"}),
        );

        assert_eq!(result["isError"], true);
        assert_eq!(
            result["structuredContent"]["status"],
            "explain_gate_rejected"
        );
        assert_eq!(
            result["structuredContent"]["explainGate"]["maxEstimatedRows"],
            1000
        );
        assert_eq!(
            result["structuredContent"]["explainGate"]["plan"][0]["accessType"],
            "ALL"
        );
    }
}
