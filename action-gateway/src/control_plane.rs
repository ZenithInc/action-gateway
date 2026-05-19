use action_gateway_v2::store::{AccessPolicyRecord, FileStore};
use axum::http::{HeaderMap, header::AUTHORIZATION};
use serde_json::{Value, json};

const DEFAULT_SOURCE_NAME: &str = "default";

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub legacy_token: Option<String>,
    pub legacy_token_allowed: bool,
    pub anonymous_local_allowed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthContext {
    pub principal_id: String,
    pub principal_type: String,
    pub principal_name: Option<String>,
    pub api_key_id: Option<String>,
    pub scopes: Value,
    pub unrestricted: bool,
}

impl AuthContext {
    pub fn legacy_admin() -> Self {
        Self {
            principal_id: "legacy:rpc-token".to_string(),
            principal_type: "legacy_admin".to_string(),
            principal_name: Some("Legacy RPC token".to_string()),
            api_key_id: None,
            scopes: json!({"legacy": true, "unrestricted": true}),
            unrestricted: true,
        }
    }

    pub fn anonymous_local() -> Self {
        Self {
            principal_id: "local:anonymous".to_string(),
            principal_type: "local_dev".to_string(),
            principal_name: Some("Local anonymous developer".to_string()),
            api_key_id: None,
            scopes: json!({"local": true, "unrestricted": true}),
            unrestricted: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthErrorKind {
    MissingBearer,
    InvalidBearer,
    InvalidApiKey,
    DisabledApiKey,
    ExpiredApiKey,
    DisabledPrincipal,
    StoreUnavailable,
}

#[derive(Debug, Clone)]
pub struct AuthError {
    pub kind: AuthErrorKind,
    pub api_key_id: Option<String>,
    pub message: String,
}

impl AuthError {
    fn new(kind: AuthErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            api_key_id: None,
            message: message.into(),
        }
    }

    fn with_key_id(mut self, api_key_id: Option<String>) -> Self {
        self.api_key_id = api_key_id;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    pub source_name: String,
}

impl SourceRef {
    #[cfg(test)]
    pub fn legacy_default() -> Self {
        Self {
            source_name: DEFAULT_SOURCE_NAME.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAuthorizationScope {
    pub source: SourceRef,
    pub tool_name: String,
    pub action_name: String,
    pub resource_type: Option<String>,
    pub resource_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessDecision {
    pub allowed: bool,
    pub decision: String,
    pub reason: String,
    pub policy_id: Option<String>,
}

impl AccessDecision {
    pub fn legacy_allowed() -> Self {
        Self {
            allowed: true,
            decision: "allowed".to_string(),
            reason: "unrestricted local or legacy context".to_string(),
            policy_id: None,
        }
    }

    pub fn default_denied() -> Self {
        Self {
            allowed: false,
            decision: "rejected".to_string(),
            reason: "no matching allow policy".to_string(),
            policy_id: None,
        }
    }

    fn denied_by_policy(policy_id: String) -> Self {
        Self {
            allowed: false,
            decision: "rejected".to_string(),
            reason: "matched deny policy".to_string(),
            policy_id: Some(policy_id),
        }
    }

    fn allowed_by_policy(policy_id: String) -> Self {
        Self {
            allowed: true,
            decision: "allowed".to_string(),
            reason: "matched allow policy".to_string(),
            policy_id: Some(policy_id),
        }
    }
}

pub async fn authenticate(
    store: &FileStore,
    headers: &HeaderMap,
    config: &AuthConfig,
) -> Result<AuthContext, AuthError> {
    let Some(bearer) = bearer_token(headers) else {
        if config.anonymous_local_allowed {
            return Ok(AuthContext::anonymous_local());
        }

        return Err(AuthError::new(
            AuthErrorKind::MissingBearer,
            "missing bearer token",
        ));
    };

    if config.legacy_token_allowed
        && config
            .legacy_token
            .as_deref()
            .is_some_and(|legacy| constant_time_eq(legacy.as_bytes(), bearer.as_bytes()))
    {
        return Ok(AuthContext::legacy_admin());
    }

    let (api_key_id, secret) = parse_api_key(bearer).ok_or_else(|| {
        AuthError::new(
            AuthErrorKind::InvalidBearer,
            "bearer token is not a gateway API key",
        )
    })?;
    let row = store
        .authenticate_api_key(api_key_id, secret)
        .await
        .map_err(|error| AuthError::new(AuthErrorKind::StoreUnavailable, error))?;
    let row = row.ok_or_else(|| {
        AuthError::new(AuthErrorKind::InvalidApiKey, "api key was not found")
            .with_key_id(Some(api_key_id.to_string()))
    })?;

    if row.key_status != "active" {
        return Err(
            AuthError::new(AuthErrorKind::DisabledApiKey, "api key is not active")
                .with_key_id(Some(row.api_key_id)),
        );
    }
    if row.expired {
        return Err(
            AuthError::new(AuthErrorKind::ExpiredApiKey, "api key is expired")
                .with_key_id(Some(row.api_key_id)),
        );
    }
    if row.principal_status != "active" {
        return Err(
            AuthError::new(AuthErrorKind::DisabledPrincipal, "principal is not active")
                .with_key_id(Some(row.api_key_id)),
        );
    }

    let unrestricted = row
        .scopes
        .get("unrestricted")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(AuthContext {
        principal_id: row.principal_id,
        principal_type: row.principal_type,
        principal_name: row.principal_name,
        api_key_id: Some(row.api_key_id),
        scopes: row.scopes,
        unrestricted,
    })
}

pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .trim()
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn parse_api_key(value: &str) -> Option<(&str, &str)> {
    let value = value.strip_prefix("agk_")?;
    let (api_key_id, secret) = value.split_once('_')?;
    if api_key_id.is_empty() || secret.is_empty() {
        return None;
    }
    if !api_key_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return None;
    }

    Some((api_key_id, secret))
}

pub fn source_ref_from_arguments(
    arguments: &Value,
    _auth: &AuthContext,
) -> Result<SourceRef, String> {
    let source_name = optional_scope_string(arguments, "source_name")?
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_SOURCE_NAME.to_string());

    validate_scope_component(&source_name, "source_name")?;

    Ok(SourceRef { source_name })
}

pub fn arguments_with_source_ref(arguments: &Value, source: &SourceRef) -> Value {
    let mut arguments = arguments.clone();
    let object = arguments
        .as_object_mut()
        .expect("arguments were checked as an object before source normalization");
    object
        .entry("source_name".to_string())
        .or_insert_with(|| Value::String(source.source_name.clone()));

    arguments
}

pub async fn authorize_tool(
    store: &FileStore,
    auth: &AuthContext,
    scope: &ToolAuthorizationScope,
) -> Result<AccessDecision, String> {
    if auth.unrestricted {
        return Ok(AccessDecision::legacy_allowed());
    }

    let policies = store
        .list_enabled_policies_for_auth(&auth.principal_id, auth.api_key_id.as_deref())
        .await;
    for policy in policies {
        if !policy_matches_scope(&policy, scope) {
            continue;
        }

        if policy.effect == "deny" {
            return Ok(AccessDecision::denied_by_policy(policy.id));
        }
        if policy.effect == "allow" {
            return Ok(AccessDecision::allowed_by_policy(policy.id));
        }
    }

    Ok(AccessDecision::default_denied())
}

pub async fn allowed_tool_names(
    store: &FileStore,
    auth: &AuthContext,
) -> Result<Option<std::collections::BTreeSet<String>>, String> {
    if auth.unrestricted {
        return Ok(None);
    }

    Ok(store
        .allowed_tool_names(&auth.principal_id, auth.api_key_id.as_deref())
        .await)
}

fn policy_matches_scope(policy: &AccessPolicyRecord, scope: &ToolAuthorizationScope) -> bool {
    optional_match(&policy.source_name, &scope.source.source_name)
        && optional_match(&policy.tool_name, &scope.tool_name)
        && optional_match(&policy.action_name, &scope.action_name)
        && optional_option_match(&policy.resource_type, scope.resource_type.as_deref())
        && resource_pattern_matches(&policy.resource_pattern, scope.resource_name.as_deref())
}

fn optional_match(policy_value: &Option<String>, actual: &str) -> bool {
    policy_value
        .as_deref()
        .is_none_or(|policy| policy == actual)
}

fn optional_option_match(policy_value: &Option<String>, actual: Option<&str>) -> bool {
    policy_value
        .as_deref()
        .is_none_or(|policy| Some(policy) == actual)
}

fn resource_pattern_matches(pattern: &Option<String>, resource_name: Option<&str>) -> bool {
    let Some(pattern) = pattern.as_deref() else {
        return true;
    };
    if pattern == "*" {
        return true;
    }
    let Some(resource_name) = resource_name else {
        return false;
    };
    wildcard_match(pattern, resource_name)
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == value;
    }

    let mut remainder = value;
    let mut first = true;
    for part in pattern.split('*') {
        if part.is_empty() {
            continue;
        }
        if first && !pattern.starts_with('*') {
            let Some(next) = remainder.strip_prefix(part) else {
                return false;
            };
            remainder = next;
            first = false;
            continue;
        }
        let Some(index) = remainder.find(part) else {
            return false;
        };
        remainder = &remainder[index + part.len()..];
        first = false;
    }

    pattern.ends_with('*') || remainder.is_empty()
}

fn optional_scope_string<'a>(arguments: &'a Value, name: &str) -> Result<Option<&'a str>, String> {
    match arguments.get(name) {
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(Value::String(_)) => Err(format!("{name} must not be empty")),
        Some(_) => Err(format!("{name} must be a string")),
        None => Ok(None),
    }
}

fn validate_scope_component(value: &str, name: &str) -> Result<(), String> {
    if value.len() > 64 {
        return Err(format!("{name} must be 64 bytes or fewer"));
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

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut diff = 0_u8;
    for (left, right) in left.iter().zip(right.iter()) {
        diff |= left ^ right;
    }

    diff == 0
}

pub fn access_decision_summary(scope: &ToolAuthorizationScope, decision: &AccessDecision) -> Value {
    json!({
        "decision": decision.decision,
        "allowed": decision.allowed,
        "reason": decision.reason,
        "policyId": decision.policy_id,
        "sourceName": scope.source.source_name,
        "tool": scope.tool_name,
        "action": scope.action_name,
        "resourceType": scope.resource_type,
        "resourceName": scope.resource_name
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gateway_api_key() {
        assert_eq!(
            parse_api_key("agk_key123_secret456"),
            Some(("key123", "secret456"))
        );
        assert_eq!(parse_api_key("legacy"), None);
        assert_eq!(parse_api_key("agk__secret"), None);
    }

    #[test]
    fn resolves_default_source_ref_from_auth() {
        let auth = AuthContext {
            principal_id: "user_1".to_string(),
            principal_type: "user".to_string(),
            principal_name: None,
            api_key_id: Some("key_1".to_string()),
            scopes: json!({}),
            unrestricted: false,
        };

        let source = source_ref_from_arguments(&json!({}), &auth).unwrap();

        assert_eq!(source.source_name, "default");
    }

    #[test]
    fn matches_resource_wildcards() {
        assert!(wildcard_match("orders:*", "orders:paid"));
        assert!(wildcard_match("*:paid", "orders:paid"));
        assert!(wildcard_match("orders:*:2026", "orders:paid:2026"));
        assert!(!wildcard_match("orders:*:2026", "orders:paid:2025"));
    }
}
