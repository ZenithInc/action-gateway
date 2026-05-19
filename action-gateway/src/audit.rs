use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use action_gateway_v2::store::{FileStore, now_string};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::control_plane;

pub const TOOL_QUERY_APPROVAL_AUDIT_EVENTS: &str = "audit.query_approval_events";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditContext {
    pub request_id: String,
    pub principal_id: Option<String>,
    pub principal_type: Option<String>,
    pub api_key_id: Option<String>,
    pub approval_id: Option<String>,
    pub action_request_id: Option<String>,
    pub actor_id: Option<String>,
    pub actor_role: Option<String>,
    pub source_ip: Option<String>,
    pub user_agent: Option<String>,
    pub service_name: Option<String>,
}

impl AuditContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: Option<String>,
        approval_id: Option<String>,
        action_request_id: Option<String>,
        actor_id: Option<String>,
        actor_role: Option<String>,
        source_ip: Option<String>,
        user_agent: Option<String>,
        service_name: Option<String>,
    ) -> Self {
        Self {
            request_id: request_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            principal_id: None,
            principal_type: None,
            api_key_id: None,
            approval_id,
            action_request_id,
            actor_id,
            actor_role,
            source_ip,
            user_agent,
            service_name,
        }
    }

    pub fn with_auth(mut self, auth: &control_plane::AuthContext) -> Self {
        self.principal_id = Some(auth.principal_id.clone());
        self.principal_type = Some(auth.principal_type.clone());
        self.api_key_id = auth.api_key_id.clone();
        if self.actor_id.is_none() {
            self.actor_id = Some(auth.principal_id.clone());
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ApprovalAuditEvent {
    id: String,
    request_id: String,
    principal_id: Option<String>,
    principal_type: Option<String>,
    api_key_id: Option<String>,
    approval_id: Option<String>,
    action_request_id: Option<String>,
    event_type: String,
    action_name: Option<String>,
    actor_id: Option<String>,
    actor_role: Option<String>,
    subject_id: Option<String>,
    target_resource: Option<String>,
    environment_name: Option<String>,
    source_name: Option<String>,
    credential_version: Option<i64>,
    before_status: Option<String>,
    after_status: Option<String>,
    decision: Option<String>,
    reason: Option<String>,
    policy_result: Option<Value>,
    request_summary: Value,
    result_summary: Option<Value>,
    request_digest: Option<String>,
    result_digest: Option<String>,
    source_ip: Option<String>,
    user_agent: Option<String>,
    service_name: Option<String>,
}

pub async fn record_mcp_tool_call(
    store: &FileStore,
    context: &AuditContext,
    request_body: &str,
    response: Option<&Value>,
) -> Result<(), String> {
    let Ok(request) = serde_json::from_str::<Value>(request_body) else {
        return Ok(());
    };
    let Some(event) = build_mcp_tool_call_event(context, &request, response) else {
        return Ok(());
    };

    insert_approval_audit_event(store, event).await
}

pub async fn record_authentication_event(
    store: &FileStore,
    context: &AuditContext,
    status: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    let decision = match status {
        "succeeded" => "allowed",
        _ => "rejected",
    };
    let request_summary = json!({
        "method": "http",
        "authScheme": "bearer",
        "principalId": context.principal_id,
        "apiKeyId": context.api_key_id,
        "actorIdHeaderPresent": context.actor_id.is_some(),
        "serviceName": context.service_name
    });
    let result_summary = json!({
        "status": status,
        "reason": reason
    });

    let event = ApprovalAuditEvent {
        id: Uuid::new_v4().to_string(),
        request_id: context.request_id.clone(),
        principal_id: context.principal_id.clone(),
        principal_type: context.principal_type.clone(),
        api_key_id: context.api_key_id.clone(),
        approval_id: context.approval_id.clone(),
        action_request_id: context.action_request_id.clone(),
        event_type: format!("auth.{status}"),
        action_name: None,
        actor_id: context.actor_id.clone(),
        actor_role: context.actor_role.clone(),
        subject_id: context.principal_id.clone(),
        target_resource: context.principal_id.clone(),
        environment_name: None,
        source_name: None,
        credential_version: None,
        before_status: None,
        after_status: Some(status.to_string()),
        decision: Some(decision.to_string()),
        reason: reason.map(|reason| reason.chars().take(2048).collect()),
        policy_result: Some(json!({"decision": decision, "status": status})),
        request_digest: Some(stable_digest(&request_summary)),
        result_digest: Some(stable_digest(&result_summary)),
        request_summary,
        result_summary: Some(result_summary),
        source_ip: context.source_ip.clone(),
        user_agent: context.user_agent.clone(),
        service_name: context.service_name.clone(),
    };

    insert_approval_audit_event(store, event).await
}

pub async fn record_authentication_failure(
    store: &FileStore,
    context: &AuditContext,
    error: &control_plane::AuthError,
) -> Result<(), String> {
    let mut context = context.clone();
    context.api_key_id = error.api_key_id.clone();
    record_authentication_event(store, &context, "failed", Some(&error.message)).await
}

pub async fn record_admin_config_event(
    store: &FileStore,
    context: &AuditContext,
    auth: &control_plane::AuthContext,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    status: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    let decision = if status == "succeeded" {
        "allowed"
    } else {
        "failed"
    };
    let request_summary = json!({
        "method": "admin",
        "action": action,
        "resourceType": resource_type,
        "resourceId": resource_id
    });
    let result_summary = json!({
        "status": status,
        "reason": reason
    });
    let event = ApprovalAuditEvent {
        id: Uuid::new_v4().to_string(),
        request_id: context.request_id.clone(),
        principal_id: Some(auth.principal_id.clone()),
        principal_type: Some(auth.principal_type.clone()),
        api_key_id: auth.api_key_id.clone(),
        approval_id: context.approval_id.clone(),
        action_request_id: context.action_request_id.clone(),
        event_type: "config.change".to_string(),
        action_name: Some(action.to_string()),
        actor_id: context
            .actor_id
            .clone()
            .or_else(|| Some(auth.principal_id.clone())),
        actor_role: context.actor_role.clone(),
        subject_id: Some(resource_id.chars().take(255).collect()),
        target_resource: Some(
            format!("{resource_type}/{resource_id}")
                .chars()
                .take(255)
                .collect(),
        ),
        environment_name: None,
        source_name: None,
        credential_version: None,
        before_status: None,
        after_status: Some(status.to_string()),
        decision: Some(decision.to_string()),
        reason: reason.map(|reason| reason.chars().take(2048).collect()),
        policy_result: Some(json!({"decision": decision, "status": status})),
        request_digest: Some(stable_digest(&request_summary)),
        result_digest: Some(stable_digest(&result_summary)),
        request_summary,
        result_summary: Some(result_summary),
        source_ip: context.source_ip.clone(),
        user_agent: context.user_agent.clone(),
        service_name: context.service_name.clone(),
    };

    insert_approval_audit_event(store, event).await
}

async fn insert_approval_audit_event(
    store: &FileStore,
    event: ApprovalAuditEvent,
) -> Result<(), String> {
    store.append_audit_event(event_to_json(event)).await
}

fn event_to_json(event: ApprovalAuditEvent) -> Value {
    json!({
        "id": event.id,
        "requestId": event.request_id,
        "principalId": event.principal_id,
        "principalType": event.principal_type,
        "apiKeyId": event.api_key_id,
        "approvalId": event.approval_id,
        "actionRequestId": event.action_request_id,
        "eventType": event.event_type,
        "actionName": event.action_name,
        "actorId": event.actor_id,
        "actorRole": event.actor_role,
        "subjectId": event.subject_id,
        "targetResource": event.target_resource,
        "environment": event.environment_name,
        "sourceName": event.source_name,
        "credentialVersion": event.credential_version,
        "beforeStatus": event.before_status,
        "afterStatus": event.after_status,
        "decision": event.decision,
        "reason": event.reason,
        "policyResult": event.policy_result,
        "requestSummary": event.request_summary,
        "resultSummary": event.result_summary,
        "requestDigest": event.request_digest,
        "resultDigest": event.result_digest,
        "sourceIp": event.source_ip,
        "userAgent": event.user_agent,
        "serviceName": event.service_name,
        "createdAt": now_string()
    })
}

fn build_mcp_tool_call_event(
    context: &AuditContext,
    request: &Value,
    response: Option<&Value>,
) -> Option<ApprovalAuditEvent> {
    if request.get("method").and_then(Value::as_str) != Some("tools/call") {
        return None;
    }

    let action_name = request
        .pointer("/params/name")
        .and_then(Value::as_str)
        .map(str::to_string);
    let request_summary = request_summary(request, action_name.as_deref());
    let result_summary = response.map(result_summary);
    let after_status = response.and_then(extract_after_status);
    let reason = response.and_then(extract_reason);
    let decision = after_status
        .as_deref()
        .map(decision_for_status)
        .map(str::to_string);
    let policy_result = policy_result(decision.as_deref(), after_status.as_deref(), response);
    let subject_id = subject_id(action_name.as_deref(), &request_summary);
    let environment_name = summary_string(&request_summary, "environment").or_else(|| {
        result_summary
            .as_ref()
            .and_then(|summary| summary_string(summary, "environment"))
    });
    let source_name = summary_string(&request_summary, "sourceName").or_else(|| {
        result_summary
            .as_ref()
            .and_then(|summary| summary_string(summary, "sourceName"))
    });
    let credential_version = result_summary
        .as_ref()
        .and_then(|summary| summary.get("credentialVersion"))
        .and_then(Value::as_i64);
    let request_digest = stable_digest(&request_summary);
    let result_digest = result_summary.as_ref().map(stable_digest);

    Some(ApprovalAuditEvent {
        id: Uuid::new_v4().to_string(),
        request_id: context.request_id.clone(),
        principal_id: context.principal_id.clone(),
        principal_type: context.principal_type.clone(),
        api_key_id: context.api_key_id.clone(),
        approval_id: context.approval_id.clone(),
        action_request_id: context.action_request_id.clone(),
        event_type: "action.tool_call".to_string(),
        action_name,
        actor_id: context.actor_id.clone(),
        actor_role: context.actor_role.clone(),
        target_resource: subject_id.clone(),
        subject_id,
        environment_name,
        source_name,
        credential_version,
        before_status: None,
        after_status,
        decision,
        reason,
        policy_result,
        request_digest: Some(request_digest),
        result_digest,
        request_summary,
        result_summary,
        source_ip: context.source_ip.clone(),
        user_agent: context.user_agent.clone(),
        service_name: context.service_name.clone(),
    })
}

fn request_summary(request: &Value, action_name: Option<&str>) -> Value {
    let arguments = request.pointer("/params/arguments").unwrap_or(&Value::Null);
    let mut summary = Map::new();
    summary.insert("method".to_string(), json!("tools/call"));
    insert_optional_value(&mut summary, "jsonrpcId", request.get("id").cloned());
    insert_optional_value(&mut summary, "action", action_name.map(Value::from));
    insert_optional_value(
        &mut summary,
        "environment",
        arguments.get("environment").cloned(),
    );

    match action_name {
        Some("data.query_table") => {
            insert_optional_value(
                &mut summary,
                "sourceName",
                string_argument(arguments, "source_name")
                    .map(Value::from)
                    .or_else(|| Some(json!("default"))),
            );
            insert_optional_value(
                &mut summary,
                "tableName",
                arguments.get("table_name").cloned(),
            );
            insert_optional_value(&mut summary, "columns", arguments.get("columns").cloned());
            summary.insert(
                "filterColumns".to_string(),
                json!(object_keys(arguments, "filters")),
            );
            insert_optional_value(&mut summary, "limit", arguments.get("limit").cloned());
        }
        Some("redis.query_key") => {
            insert_optional_value(
                &mut summary,
                "sourceName",
                string_argument(arguments, "source_name")
                    .map(Value::from)
                    .or_else(|| Some(json!("default"))),
            );
            insert_optional_value(&mut summary, "key", arguments.get("key").cloned());
            insert_optional_value(&mut summary, "limit", arguments.get("limit").cloned());
        }
        Some("kubernetes.kubectl_read") => {
            insert_optional_value(
                &mut summary,
                "sourceName",
                string_argument(arguments, "source_name")
                    .map(Value::from)
                    .or_else(|| Some(json!("default"))),
            );
            insert_optional_value(&mut summary, "args", arguments.get("args").cloned());
            insert_optional_value(
                &mut summary,
                "timeoutSeconds",
                arguments.get("timeout_seconds").cloned(),
            );
            insert_optional_value(
                &mut summary,
                "maxOutputBytes",
                arguments.get("max_output_bytes").cloned(),
            );
        }
        Some("kubernetes.list_resources") => {
            insert_optional_value(
                &mut summary,
                "sourceName",
                string_argument(arguments, "source_name")
                    .map(Value::from)
                    .or_else(|| Some(json!("default"))),
            );
            insert_optional_value(
                &mut summary,
                "namespace",
                arguments.get("namespace").cloned(),
            );
            insert_optional_value(&mut summary, "resource", arguments.get("resource").cloned());
            insert_optional_value(
                &mut summary,
                "labelSelector",
                arguments.get("label_selector").cloned(),
            );
            insert_optional_value(
                &mut summary,
                "fieldSelector",
                arguments.get("field_selector").cloned(),
            );
            insert_optional_value(&mut summary, "limit", arguments.get("limit").cloned());
        }
        Some("kubernetes.get_resource") => {
            insert_optional_value(
                &mut summary,
                "sourceName",
                string_argument(arguments, "source_name")
                    .map(Value::from)
                    .or_else(|| Some(json!("default"))),
            );
            insert_optional_value(
                &mut summary,
                "namespace",
                arguments.get("namespace").cloned(),
            );
            insert_optional_value(&mut summary, "resource", arguments.get("resource").cloned());
            insert_optional_value(&mut summary, "name", arguments.get("name").cloned());
        }
        Some("kubernetes.rollout_status") => {
            insert_optional_value(
                &mut summary,
                "sourceName",
                string_argument(arguments, "source_name")
                    .map(Value::from)
                    .or_else(|| Some(json!("default"))),
            );
            insert_optional_value(
                &mut summary,
                "namespace",
                arguments.get("namespace").cloned(),
            );
            insert_optional_value(&mut summary, "resource", arguments.get("resource").cloned());
            insert_optional_value(&mut summary, "name", arguments.get("name").cloned());
            insert_optional_value(
                &mut summary,
                "actionType",
                arguments
                    .get("action")
                    .cloned()
                    .or_else(|| Some(json!("status"))),
            );
            insert_optional_value(&mut summary, "revision", arguments.get("revision").cloned());
        }
        Some("kubernetes.query_pod_logs") => {
            insert_optional_value(
                &mut summary,
                "sourceName",
                string_argument(arguments, "source_name")
                    .map(Value::from)
                    .or_else(|| Some(json!("default"))),
            );
            insert_optional_value(
                &mut summary,
                "namespace",
                arguments.get("namespace").cloned(),
            );
            insert_optional_value(&mut summary, "podName", arguments.get("pod_name").cloned());
            insert_optional_value(
                &mut summary,
                "container",
                arguments.get("container").cloned(),
            );
            insert_optional_value(&mut summary, "since", arguments.get("since").cloned());
            insert_optional_value(&mut summary, "previous", arguments.get("previous").cloned());
            insert_optional_value(
                &mut summary,
                "timestamps",
                arguments.get("timestamps").cloned(),
            );
            insert_optional_value(
                &mut summary,
                "tailLines",
                arguments.get("tail_lines").cloned(),
            );
        }
        Some("logs.query_app_logs") => {
            insert_optional_value(&mut summary, "appName", arguments.get("app_name").cloned());
            insert_optional_value(
                &mut summary,
                "environment",
                arguments.get("environment").cloned(),
            );
            insert_optional_value(&mut summary, "traceId", arguments.get("trace_id").cloned());
            summary.insert(
                "keywordPresent".to_string(),
                Value::Bool(arguments.get("keyword").is_some()),
            );
            insert_optional_value(&mut summary, "since", arguments.get("since").cloned());
            insert_optional_value(&mut summary, "limit", arguments.get("limit").cloned());
        }
        Some(TOOL_QUERY_APPROVAL_AUDIT_EVENTS) => {
            for field in [
                "request_id",
                "approval_id",
                "action_request_id",
                "event_type",
                "action_name",
                "principal_id",
                "api_key_id",
                "environment",
                "source_name",
                "actor_id",
                "after_status",
                "decision",
                "limit",
            ] {
                insert_optional_value(
                    &mut summary,
                    camel_case_key(field),
                    arguments.get(field).cloned(),
                );
            }
        }
        _ => {}
    }

    summary.insert("argumentKeys".to_string(), json!(argument_keys(arguments)));
    Value::Object(summary)
}

fn result_summary(response: &Value) -> Value {
    let Some(result) = response.get("result") else {
        return json!({
            "jsonrpcError": response.get("error").cloned().unwrap_or(Value::Null)
        });
    };

    let mut summary = Map::new();
    insert_optional_value(&mut summary, "isError", result.get("isError").cloned());
    let structured = result.get("structuredContent").unwrap_or(&Value::Null);

    for field in [
        "status",
        "action",
        "message",
        "environment",
        "sourceName",
        "credentialVersion",
        "tableName",
        "columns",
        "limit",
        "rowCount",
        "explainGate",
        "masking",
        "key",
        "keyType",
        "ttlSeconds",
        "exists",
        "valueLength",
        "fieldCount",
        "memberCount",
        "returnedCount",
        "truncated",
        "allowlist",
        "resource",
        "name",
        "actionType",
        "revision",
        "totalItems",
        "namespace",
        "podName",
        "container",
        "since",
        "tailLines",
        "lineCount",
        "command",
        "args",
        "exitCode",
        "timedOut",
        "timeoutSeconds",
        "maxOutputBytes",
        "stdoutTruncated",
        "stderrTruncated",
        "logsTruncated",
        "outputTruncated",
        "eventCount",
        "appName",
        "environment",
        "traceId",
        "keywordPresent",
        "scannedCount",
        "authorization",
    ] {
        insert_optional_value(&mut summary, field, structured.get(field).cloned());
    }

    Value::Object(summary)
}

fn policy_result(
    decision: Option<&str>,
    after_status: Option<&str>,
    response: Option<&Value>,
) -> Option<Value> {
    let mut policy = Map::new();
    insert_optional_value(&mut policy, "decision", decision.map(Value::from));
    insert_optional_value(&mut policy, "status", after_status.map(Value::from));

    let structured = response
        .and_then(|response| response.get("result"))
        .and_then(|result| result.get("structuredContent"));
    if let Some(explain_gate) = structured.and_then(|structured| structured.get("explainGate")) {
        policy.insert("explainGate".to_string(), explain_gate.clone());
    }
    if let Some(allowlist) = structured.and_then(|structured| structured.get("allowlist")) {
        policy.insert("allowlist".to_string(), allowlist.clone());
    }
    if let Some(authorization) = structured.and_then(|structured| structured.get("authorization")) {
        policy.insert("authorization".to_string(), authorization.clone());
    }

    (!policy.is_empty()).then_some(Value::Object(policy))
}

fn extract_after_status(response: &Value) -> Option<String> {
    if response.get("error").is_some() {
        return Some("protocol_error".to_string());
    }

    response
        .pointer("/result/structuredContent/status")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            response
                .pointer("/result/isError")
                .and_then(Value::as_bool)
                .map(|is_error| {
                    if is_error {
                        "failed".to_string()
                    } else {
                        "succeeded".to_string()
                    }
                })
        })
}

fn extract_reason(response: &Value) -> Option<String> {
    response
        .pointer("/result/structuredContent/message")
        .and_then(Value::as_str)
        .or_else(|| response.pointer("/error/message").and_then(Value::as_str))
        .map(|value| value.chars().take(2048).collect())
}

fn decision_for_status(status: &str) -> &'static str {
    match status {
        "succeeded" => "allowed",
        "invalid_arguments" | "not_allowed" | "explain_gate_rejected" | "protocol_error" => {
            "rejected"
        }
        _ => "failed",
    }
}

fn subject_id(action_name: Option<&str>, request_summary: &Value) -> Option<String> {
    let field = match action_name {
        Some("data.query_table") => "tableName",
        Some("redis.query_key") => "key",
        Some("kubernetes.list_resources") => {
            return kubernetes_subject_from_summary(request_summary, "*", "list");
        }
        Some("kubernetes.get_resource") => {
            return kubernetes_subject_from_summary(request_summary, "", "get");
        }
        Some("kubernetes.rollout_status") => {
            let action = match request_summary.get("actionType").and_then(Value::as_str) {
                Some("history") => "rollout_history",
                _ => "rollout_status",
            };
            return kubernetes_subject_from_summary(request_summary, "", action);
        }
        Some("kubernetes.query_pod_logs") => {
            return kubernetes_subject(
                request_summary.get("namespace").and_then(Value::as_str),
                Some("pods"),
                request_summary.get("podName").and_then(Value::as_str),
                "logs",
            );
        }
        Some("logs.query_app_logs") => "appName",
        Some("kubernetes.kubectl_read") => {
            return kubernetes_subject_from_raw_args(request_summary.get("args"));
        }
        Some(TOOL_QUERY_APPROVAL_AUDIT_EVENTS) => return Some("approval_audit_events".to_string()),
        _ => return None,
    };

    let value = request_summary.get(field)?;
    if let Some(value) = value.as_str() {
        return Some(value.chars().take(255).collect());
    }
    if let Some(values) = value.as_array() {
        return Some(
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(255)
                .collect(),
        );
    }

    None
}

fn kubernetes_subject_from_summary(
    request_summary: &Value,
    default_name: &str,
    action: &str,
) -> Option<String> {
    let name = request_summary
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| (!default_name.is_empty()).then_some(default_name));
    kubernetes_subject(
        request_summary.get("namespace").and_then(Value::as_str),
        request_summary.get("resource").and_then(Value::as_str),
        name,
        action,
    )
}

fn kubernetes_subject(
    namespace: Option<&str>,
    resource: Option<&str>,
    name: Option<&str>,
    action: &str,
) -> Option<String> {
    let namespace = namespace?;
    let resource = resource?;
    let name = name.unwrap_or("*");

    Some(
        format!(
            "{}/{}/{}/{}",
            truncate_subject_part(namespace),
            truncate_subject_part(&normalize_kubernetes_resource_name(resource)),
            truncate_subject_part(name),
            truncate_subject_part(action)
        )
        .chars()
        .take(255)
        .collect(),
    )
}

fn kubernetes_subject_from_raw_args(args: Option<&Value>) -> Option<String> {
    let args = args?.as_array()?;
    let args = args.iter().filter_map(Value::as_str).collect::<Vec<_>>();
    let command = args.first().copied()?;
    match command {
        "get" => {
            let namespace = raw_kubernetes_namespace(&args)?;
            let positionals = raw_kubernetes_positionals(&args, 1);
            let (resource, name) = raw_kubernetes_resource_and_name(&positionals)?;
            let action = if name.is_some() { "get" } else { "list" };
            kubernetes_subject(Some(namespace), Some(&resource), name.as_deref(), action)
        }
        "describe" => {
            let namespace = raw_kubernetes_namespace(&args)?;
            let positionals = raw_kubernetes_positionals(&args, 1);
            let (resource, name) = raw_kubernetes_resource_and_name(&positionals)?;
            kubernetes_subject(
                Some(namespace),
                Some(&resource),
                name.as_deref(),
                "describe",
            )
        }
        "rollout" => {
            let namespace = raw_kubernetes_namespace(&args)?;
            let subcommand = args.get(1).copied().unwrap_or("status");
            let positionals = raw_kubernetes_positionals(&args, 2);
            let (resource, name) = raw_kubernetes_resource_and_name(&positionals)?;
            let action = match subcommand {
                "history" => "rollout_history",
                _ => "rollout_status",
            };
            kubernetes_subject(Some(namespace), Some(&resource), name.as_deref(), action)
        }
        "api-resources" | "api-versions" | "version" => Some(format!("cluster/*/*/{command}")),
        "config" => args
            .get(1)
            .map(|subcommand| format!("cluster/*/*/config_{}", truncate_subject_part(subcommand))),
        _ => None,
    }
}

fn raw_kubernetes_namespace<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut index = 0_usize;
    while index < args.len() {
        match args[index] {
            "-n" | "--namespace" => return args.get(index + 1).copied(),
            arg => {
                if let Some(namespace) = arg.strip_prefix("--namespace=") {
                    return Some(namespace);
                }
            }
        }
        index += 1;
    }

    None
}

fn raw_kubernetes_positionals(args: &[&str], start: usize) -> Vec<String> {
    let mut positionals = Vec::new();
    let mut index = start;
    while index < args.len() {
        let arg = args[index];
        if raw_kubernetes_flag_takes_value(arg) {
            index += if arg.contains('=') { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        positionals.push(arg.to_string());
        index += 1;
    }

    positionals
}

fn raw_kubernetes_flag_takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "-n" | "--namespace" | "-o" | "--output" | "-l" | "--selector" | "--field-selector"
    ) || arg.starts_with("--namespace=")
        || arg.starts_with("--output=")
        || arg.starts_with("-o=")
        || arg.starts_with("--selector=")
        || arg.starts_with("--field-selector=")
}

fn raw_kubernetes_resource_and_name(positionals: &[String]) -> Option<(String, Option<String>)> {
    let resource = positionals.first()?;
    if let Some((resource, name)) = resource.split_once('/') {
        return Some((
            normalize_kubernetes_resource_name(resource),
            Some(name.to_string()),
        ));
    }

    Some((
        normalize_kubernetes_resource_name(resource),
        positionals.get(1).cloned(),
    ))
}

fn normalize_kubernetes_resource_name(resource: &str) -> String {
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

fn truncate_subject_part(value: &str) -> String {
    value.chars().take(96).collect()
}

fn insert_optional_value<K>(map: &mut Map<String, Value>, key: K, value: Option<Value>)
where
    K: Into<String>,
{
    if let Some(value) = value
        && !value.is_null()
    {
        map.insert(key.into(), value);
    }
}

fn summary_string(summary: &Value, field: &str) -> Option<String> {
    summary
        .get(field)
        .and_then(Value::as_str)
        .map(|value| value.chars().take(255).collect())
}

fn stable_digest(value: &Value) -> String {
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(value)
        .unwrap_or_else(|_| "null".to_string())
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn string_argument<'a>(arguments: &'a Value, name: &str) -> Option<&'a str> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn argument_keys(arguments: &Value) -> Vec<String> {
    let mut keys = arguments
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn object_keys(arguments: &Value, name: &str) -> Vec<String> {
    let mut keys = arguments
        .get(name)
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn camel_case_key(field: &str) -> String {
    let mut key = String::with_capacity(field.len());
    let mut uppercase_next = false;

    for character in field.chars() {
        if character == '_' {
            uppercase_next = true;
            continue;
        }
        if uppercase_next {
            key.push(character.to_ascii_uppercase());
            uppercase_next = false;
        } else {
            key.push(character);
        }
    }

    key
}

pub fn tool_definition() -> Value {
    json!({
        "name": TOOL_QUERY_APPROVAL_AUDIT_EVENTS,
        "title": "Query Approval Audit Events",
        "description": "Query approval and action audit events captured by the gateway. Returned records are summaries and do not include full business rows, logs, stdout, or Redis values.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "request_id": {
                    "type": "string",
                    "description": "Filter by gateway request id."
                },
                "approval_id": {
                    "type": "string",
                    "description": "Filter by approval id when provided by the caller."
                },
                "action_request_id": {
                    "type": "string",
                    "description": "Filter by action request id when provided by the caller."
                },
                "event_type": {
                    "type": "string",
                    "description": "Filter by event type, such as action.tool_call."
                },
                "action_name": {
                    "type": "string",
                    "description": "Filter by MCP tool/action name."
                },
                "principal_id": {
                    "type": "string",
                    "description": "Filter by authenticated Gateway principal id."
                },
                "api_key_id": {
                    "type": "string",
                    "description": "Filter by Gateway API key id."
                },
                "environment": {
                    "type": "string",
                    "description": "Filter by environment name."
                },
                "source_name": {
                    "type": "string",
                    "description": "Filter by source alias."
                },
                "actor_id": {
                    "type": "string",
                    "description": "Filter by actor id captured from headers."
                },
                "after_status": {
                    "type": "string",
                    "description": "Filter by resulting status."
                },
                "decision": {
                    "type": "string",
                    "description": "Filter by normalized decision, such as allowed, rejected, or failed."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1000,
                    "default": 100
                }
            },
            "additionalProperties": false
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct AuditQueryFilters {
    request_id: Option<String>,
    approval_id: Option<String>,
    action_request_id: Option<String>,
    event_type: Option<String>,
    action_name: Option<String>,
    principal_id: Option<String>,
    api_key_id: Option<String>,
    environment: Option<String>,
    source_name: Option<String>,
    actor_id: Option<String>,
    after_status: Option<String>,
    decision: Option<String>,
    limit: i64,
}

pub async fn query_approval_audit_events(store: &FileStore, arguments: &Value) -> Value {
    let filters = match parse_audit_query_filters(arguments) {
        Ok(filters) => filters,
        Err(message) => return audit_tool_argument_error(&message, arguments),
    };

    let events = fetch_approval_audit_events(store, &filters).await;
    audit_query_result(events, arguments)
}

fn parse_audit_query_filters(arguments: &Value) -> Result<AuditQueryFilters, String> {
    if !arguments.is_object() {
        return Err("arguments must be an object".to_string());
    }

    Ok(AuditQueryFilters {
        request_id: optional_filter_string(arguments, "request_id")?,
        approval_id: optional_filter_string(arguments, "approval_id")?,
        action_request_id: optional_filter_string(arguments, "action_request_id")?,
        event_type: optional_filter_string(arguments, "event_type")?,
        action_name: optional_filter_string(arguments, "action_name")?,
        principal_id: optional_filter_string(arguments, "principal_id")?,
        api_key_id: optional_filter_string(arguments, "api_key_id")?,
        environment: optional_filter_string(arguments, "environment")?,
        source_name: optional_filter_string(arguments, "source_name")?,
        actor_id: optional_filter_string(arguments, "actor_id")?,
        after_status: optional_filter_string(arguments, "after_status")?,
        decision: optional_filter_string(arguments, "decision")?,
        limit: parse_limit(arguments)?,
    })
}

fn optional_filter_string(arguments: &Value, name: &str) -> Result<Option<String>, String> {
    match arguments.get(name) {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= 255 => {
            Ok(Some(value.to_string()))
        }
        Some(Value::String(value)) if value.is_empty() => Err(format!("{name} must not be empty")),
        Some(Value::String(_)) => Err(format!("{name} must be 255 bytes or fewer")),
        Some(_) => Err(format!("{name} must be a string")),
        None => Ok(None),
    }
}

fn parse_limit(arguments: &Value) -> Result<i64, String> {
    let limit = match arguments.get("limit") {
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| "limit must be a positive integer".to_string())?,
        Some(_) => return Err("limit must be a positive integer".to_string()),
        None => 100,
    };
    if !(1..=1000).contains(&limit) {
        return Err("limit must be between 1 and 1000".to_string());
    }

    Ok(limit)
}

async fn fetch_approval_audit_events(store: &FileStore, filters: &AuditQueryFilters) -> Vec<Value> {
    let mut events = store
        .audit_events()
        .await
        .into_iter()
        .filter(|event| audit_event_matches(event, filters))
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        right
            .get("createdAt")
            .and_then(Value::as_str)
            .cmp(&left.get("createdAt").and_then(Value::as_str))
            .then_with(|| {
                right
                    .get("id")
                    .and_then(Value::as_str)
                    .cmp(&left.get("id").and_then(Value::as_str))
            })
    });
    events.truncate(usize::try_from(filters.limit).unwrap_or(100));
    events
}

fn audit_event_matches(event: &Value, filters: &AuditQueryFilters) -> bool {
    filter_matches(event, "requestId", &filters.request_id)
        && filter_matches(event, "approvalId", &filters.approval_id)
        && filter_matches(event, "actionRequestId", &filters.action_request_id)
        && filter_matches(event, "eventType", &filters.event_type)
        && filter_matches(event, "actionName", &filters.action_name)
        && filter_matches(event, "principalId", &filters.principal_id)
        && filter_matches(event, "apiKeyId", &filters.api_key_id)
        && filter_matches(event, "environment", &filters.environment)
        && filter_matches(event, "sourceName", &filters.source_name)
        && filter_matches(event, "actorId", &filters.actor_id)
        && filter_matches(event, "afterStatus", &filters.after_status)
        && filter_matches(event, "decision", &filters.decision)
}

fn filter_matches(event: &Value, key: &str, expected: &Option<String>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    event.get(key).and_then(Value::as_str) == Some(expected.as_str())
}

fn audit_query_result(events: Vec<Value>, arguments: &Value) -> Value {
    let event_count = events.len();

    json!({
        "content": [
            {
                "type": "text",
                "text": format!("{TOOL_QUERY_APPROVAL_AUDIT_EVENTS}: returned {event_count} audit event(s)")
            }
        ],
        "structuredContent": {
            "status": "succeeded",
            "action": TOOL_QUERY_APPROVAL_AUDIT_EVENTS,
            "eventCount": event_count,
            "events": events,
            "receivedArguments": arguments
        },
        "isError": false
    })
}

fn audit_tool_error(status: &str, message: &str, arguments: &Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": format!("{TOOL_QUERY_APPROVAL_AUDIT_EVENTS}: {message}")
            }
        ],
        "structuredContent": {
            "status": status,
            "action": TOOL_QUERY_APPROVAL_AUDIT_EVENTS,
            "message": message,
            "receivedArguments": arguments
        },
        "isError": true
    })
}

fn audit_tool_argument_error(message: &str, arguments: &Value) -> Value {
    audit_tool_error("invalid_arguments", message, arguments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> AuditContext {
        AuditContext::new(
            Some("req_1".to_string()),
            Some("apr_1".to_string()),
            None,
            Some("user_1".to_string()),
            Some("admin".to_string()),
            Some("127.0.0.1".to_string()),
            Some("test-agent".to_string()),
            Some("integration-test".to_string()),
        )
    }

    #[test]
    fn builds_table_query_audit_event_without_row_data() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "data.query_table",
                "arguments": {
                    "table_name": "orders",
                    "filters": {
                        "status": "paid"
                    },
                    "limit": 10
                }
            }
        });
        let response = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": {
                "structuredContent": {
                    "status": "succeeded",
                    "action": "data.query_table",
                    "tableName": "orders",
                    "rowCount": 1,
                    "rows": [{"id": 1}],
                    "explainGate": {
                        "maxEstimatedRows": 1000,
                        "estimatedRows": 1,
                        "passed": true
                    }
                },
                "isError": false
            }
        });

        let event = build_mcp_tool_call_event(&context(), &request, Some(&response))
            .expect("tool call should produce an audit event");

        assert_eq!(event.request_id, "req_1");
        assert_eq!(event.approval_id.as_deref(), Some("apr_1"));
        assert_eq!(event.action_name.as_deref(), Some("data.query_table"));
        assert_eq!(event.subject_id.as_deref(), Some("orders"));
        assert_eq!(event.after_status.as_deref(), Some("succeeded"));
        assert_eq!(event.decision.as_deref(), Some("allowed"));
        assert_eq!(event.request_summary["filterColumns"], json!(["status"]));
        assert!(event.result_summary.unwrap().get("rows").is_none());
    }

    #[test]
    fn summarizes_protocol_errors_as_rejected() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "missing.tool",
                "arguments": {}
            }
        });
        let response = json!({
            "jsonrpc": "2.0",
            "id": 8,
            "error": {
                "code": -32602,
                "message": "unknown tool"
            }
        });

        let event = build_mcp_tool_call_event(&context(), &request, Some(&response))
            .expect("tool call should produce an audit event");

        assert_eq!(event.after_status.as_deref(), Some("protocol_error"));
        assert_eq!(event.decision.as_deref(), Some("rejected"));
        assert_eq!(event.reason.as_deref(), Some("unknown tool"));
    }

    #[test]
    fn summarizes_kubernetes_tool_calls_without_output_payloads() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "kubernetes.query_pod_logs",
                "arguments": {
                    "namespace": "sample-prod",
                    "pod_name": "orders-api-abc123",
                    "tail_lines": 200
                }
            }
        });
        let response = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "result": {
                "structuredContent": {
                    "status": "succeeded",
                    "action": "kubernetes.query_pod_logs",
                    "namespace": "sample-prod",
                    "podName": "orders-api-abc123",
                    "tailLines": 200,
                    "lineCount": 2,
                    "logs": "first\nsecond\n",
                    "stderr": "",
                    "allowlist": {
                        "namespace": "sample-prod",
                        "resource": "pods"
                    }
                },
                "isError": false
            }
        });

        let event = build_mcp_tool_call_event(&context(), &request, Some(&response))
            .expect("tool call should produce an audit event");
        let result_summary = event.result_summary.unwrap();

        assert_eq!(
            event.subject_id.as_deref(),
            Some("sample-prod/pods/orders-api-abc123/logs")
        );
        assert_eq!(result_summary["lineCount"], 2);
        assert!(result_summary.get("logs").is_none());
        assert!(result_summary.get("stderr").is_none());
    }

    #[test]
    fn summarizes_app_log_tool_calls_without_log_payloads() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "logs.query_app_logs",
                "arguments": {
                    "app_name": "billing-api",
                    "environment": "prod",
                    "keyword": "12.00",
                    "limit": 5
                }
            }
        });
        let response = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "result": {
                "structuredContent": {
                    "status": "succeeded",
                    "action": "logs.query_app_logs",
                    "appName": "billing-api",
                    "environment": "prod",
                    "keywordPresent": true,
                    "returnedCount": 1,
                    "truncated": false,
                    "logs": [
                        {
                            "id": "log_1001",
                            "message": "paid summary returned 12.00 for customer order page"
                        }
                    ]
                },
                "isError": false
            }
        });

        let event = build_mcp_tool_call_event(&context(), &request, Some(&response))
            .expect("tool call should produce an audit event");
        let result_summary = event.result_summary.unwrap();

        assert_eq!(event.subject_id.as_deref(), Some("billing-api"));
        assert_eq!(event.request_summary["keywordPresent"], true);
        assert_eq!(result_summary["returnedCount"], 1);
        assert_eq!(result_summary["appName"], "billing-api");
        assert!(result_summary.get("logs").is_none());
        assert!(result_summary.get("message").is_none());
    }

    #[test]
    fn ignores_non_tool_call_requests() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize"
        });

        assert!(build_mcp_tool_call_event(&context(), &request, None).is_none());
    }

    #[test]
    fn validates_audit_query_limit() {
        assert_eq!(parse_audit_query_filters(&json!({})).unwrap().limit, 100);
        assert_eq!(
            parse_audit_query_filters(&json!({"limit": 1001})).unwrap_err(),
            "limit must be between 1 and 1000"
        );
    }
}
