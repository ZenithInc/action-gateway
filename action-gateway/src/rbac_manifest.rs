use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const API_VERSION: &str = "gateway.zenithinc.dev/v1";
pub const MANAGED_BY: &str = "agctl";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Metadata {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PrincipalManifest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: PrincipalSpec,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PrincipalSpec {
    #[serde(rename = "type")]
    pub principal_type: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default = "empty_json_object")]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RoleManifest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: RoleSpec,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RoleSpec {
    #[serde(default)]
    pub scope: RoleScope,
    #[serde(default)]
    pub rules: Vec<RoleRule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct RoleScope {
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RoleRule {
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub verbs: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(rename = "resourceNames", default)]
    pub resource_names: Vec<String>,
    #[serde(default = "default_allow")]
    pub effect: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RoleBindingManifest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: RoleBindingSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RoleBindingSpec {
    pub principal: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ApiKeyManifest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: ApiKeySpec,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ApiKeySpec {
    pub principal: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default = "empty_json_object")]
    pub scopes: Value,
    #[serde(rename = "expiresAt", default)]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GatewayConfig {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "currentContext")]
    pub current_context: String,
    pub contexts: Vec<GatewayContext>,
    pub credentials: GatewayCredentials,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GatewayContext {
    pub name: String,
    pub endpoint: String,
    pub principal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GatewayCredentials {
    pub token: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ManifestSet {
    pub principals: BTreeMap<String, PrincipalManifest>,
    pub roles: BTreeMap<String, RoleManifest>,
    pub role_bindings: BTreeMap<String, RoleBindingManifest>,
    pub api_keys: BTreeMap<String, ApiKeyManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PrincipalRequest {
    pub id: String,
    #[serde(rename = "principalType")]
    pub principal_type: String,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub status: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ApiKeyRequest {
    #[serde(rename = "principalId")]
    pub principal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub scopes: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AccessPolicyRequest {
    pub id: String,
    #[serde(rename = "principalId", skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<String>,
    #[serde(rename = "apiKeyId", skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<String>,
    pub effect: String,
    #[serde(rename = "sourceName", skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(rename = "toolName", skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(rename = "actionName", skip_serializing_if = "Option::is_none")]
    pub action_name: Option<String>,
    #[serde(rename = "resourceType", skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(rename = "resourcePattern", skip_serializing_if = "Option::is_none")]
    pub resource_pattern: Option<String>,
    pub constraints: Value,
    pub specificity: i64,
    pub enabled: bool,
}

impl ManifestSet {
    pub fn from_yaml_str(input: &str) -> Result<Self, String> {
        let mut set = ManifestSet::default();

        for document in serde_yaml::Deserializer::from_str(input) {
            let value = serde_yaml::Value::deserialize(document)
                .map_err(|error| format!("failed to parse YAML document: {error}"))?;
            if value.is_null() {
                continue;
            }
            let kind = value
                .get("kind")
                .and_then(serde_yaml::Value::as_str)
                .ok_or_else(|| "manifest document is missing kind".to_string())?;
            match kind {
                "Principal" => {
                    let manifest: PrincipalManifest = serde_yaml::from_value(value)
                        .map_err(|error| format!("invalid Principal manifest: {error}"))?;
                    set.insert_principal(manifest)?;
                }
                "Role" => {
                    let manifest: RoleManifest = serde_yaml::from_value(value)
                        .map_err(|error| format!("invalid Role manifest: {error}"))?;
                    set.insert_role(manifest)?;
                }
                "RoleBinding" => {
                    let manifest: RoleBindingManifest = serde_yaml::from_value(value)
                        .map_err(|error| format!("invalid RoleBinding manifest: {error}"))?;
                    set.insert_role_binding(manifest)?;
                }
                "ApiKey" => {
                    let manifest: ApiKeyManifest = serde_yaml::from_value(value)
                        .map_err(|error| format!("invalid ApiKey manifest: {error}"))?;
                    set.insert_api_key(manifest)?;
                }
                other => return Err(format!("unsupported manifest kind {other}")),
            }
        }

        set.validate()?;
        Ok(set)
    }

    pub fn principal_requests(&self) -> Vec<PrincipalRequest> {
        self.principals
            .values()
            .map(|manifest| PrincipalRequest {
                id: manifest.metadata.name.clone(),
                principal_type: manifest.spec.principal_type.clone(),
                display_name: empty_string_to_none(manifest.spec.display_name.clone()),
                status: manifest.spec.status.clone(),
                metadata: manifest.spec.metadata.clone(),
            })
            .collect()
    }

    pub fn api_key_requests(&self) -> Vec<ApiKeyRequest> {
        self.api_keys
            .values()
            .map(|manifest| ApiKeyRequest {
                principal_id: manifest.spec.principal.clone(),
                id: Some(manifest.metadata.name.clone()),
                name: empty_string_to_none(manifest.spec.display_name.clone()),
                expires_at: empty_string_to_none(manifest.spec.expires_at.clone()),
                scopes: manifest.spec.scopes.clone(),
            })
            .collect()
    }

    pub fn compile_policies(&self) -> Result<Vec<AccessPolicyRequest>, String> {
        let mut policies = Vec::new();
        for binding in self.role_bindings.values() {
            let principal_name = &binding.spec.principal;
            let role = self.roles.get(&binding.spec.role).ok_or_else(|| {
                format!(
                    "RoleBinding {} references missing Role {}",
                    binding.metadata.name, binding.spec.role
                )
            })?;

            for (rule_index, rule) in role.spec.rules.iter().enumerate() {
                let tool_name = rule
                    .tools
                    .first()
                    .ok_or_else(|| {
                        format!("Role {} rule {rule_index} has no tools", role.metadata.name)
                    })?
                    .clone();
                let action_name = rule
                    .verbs
                    .first()
                    .ok_or_else(|| {
                        format!("Role {} rule {rule_index} has no verbs", role.metadata.name)
                    })?
                    .clone();
                let resource_type = rule
                    .resources
                    .first()
                    .ok_or_else(|| {
                        format!(
                            "Role {} rule {rule_index} has no resources",
                            role.metadata.name
                        )
                    })?
                    .clone();

                for resource_name in &rule.resource_names {
                    policies.push(AccessPolicyRequest {
                        id: policy_id(&binding.metadata.name, rule_index, resource_name)?,
                        principal_id: Some(principal_name.clone()),
                        api_key_id: None,
                        effect: rule.effect.clone(),
                        source_name: normalize_optional_scope(role.spec.scope.source.clone()),
                        tool_name: Some(tool_name.clone()),
                        action_name: Some(action_name.clone()),
                        resource_type: Some(resource_type.clone()),
                        resource_pattern: Some(resource_name.clone()),
                        constraints: json!({
                            "managedBy": MANAGED_BY,
                            "role": role.metadata.name,
                            "roleBinding": binding.metadata.name,
                            "ruleIndex": rule_index,
                        }),
                        specificity: 0,
                        enabled: true,
                    });
                }
            }
        }
        Ok(policies)
    }

    pub fn role_binding_names(&self) -> BTreeSet<String> {
        self.role_bindings.keys().cloned().collect()
    }

    fn insert_principal(&mut self, manifest: PrincipalManifest) -> Result<(), String> {
        check_api_version(&manifest.api_version, "Principal", &manifest.metadata.name)?;
        insert_unique(
            &mut self.principals,
            manifest.metadata.name.clone(),
            manifest,
            "Principal",
        )
    }

    fn insert_role(&mut self, manifest: RoleManifest) -> Result<(), String> {
        check_api_version(&manifest.api_version, "Role", &manifest.metadata.name)?;
        insert_unique(
            &mut self.roles,
            manifest.metadata.name.clone(),
            manifest,
            "Role",
        )
    }

    fn insert_role_binding(&mut self, manifest: RoleBindingManifest) -> Result<(), String> {
        check_api_version(
            &manifest.api_version,
            "RoleBinding",
            &manifest.metadata.name,
        )?;
        insert_unique(
            &mut self.role_bindings,
            manifest.metadata.name.clone(),
            manifest,
            "RoleBinding",
        )
    }

    fn insert_api_key(&mut self, manifest: ApiKeyManifest) -> Result<(), String> {
        check_api_version(&manifest.api_version, "ApiKey", &manifest.metadata.name)?;
        insert_unique(
            &mut self.api_keys,
            manifest.metadata.name.clone(),
            manifest,
            "ApiKey",
        )
    }

    fn validate(&self) -> Result<(), String> {
        for principal in self.principals.values() {
            validate_name(&principal.metadata.name, "Principal")?;
            validate_enum(
                &principal.spec.principal_type,
                &["user", "service_account", "legacy_admin"],
                &format!("Principal {} spec.type", principal.metadata.name),
            )?;
            validate_enum(
                &principal.spec.status,
                &["active", "disabled"],
                &format!("Principal {} spec.status", principal.metadata.name),
            )?;
            ensure_object(
                &principal.spec.metadata,
                &format!("Principal {} spec.metadata", principal.metadata.name),
            )?;
        }

        for role in self.roles.values() {
            validate_name(&role.metadata.name, "Role")?;
            if role.spec.rules.is_empty() {
                return Err(format!(
                    "Role {} must contain at least one rule",
                    role.metadata.name
                ));
            }
            for (index, rule) in role.spec.rules.iter().enumerate() {
                validate_singleton(
                    &rule.tools,
                    &format!("Role {} rule {index} tools", role.metadata.name),
                )?;
                validate_singleton(
                    &rule.verbs,
                    &format!("Role {} rule {index} verbs", role.metadata.name),
                )?;
                validate_singleton(
                    &rule.resources,
                    &format!("Role {} rule {index} resources", role.metadata.name),
                )?;
                let resource = &rule.resources[0];
                validate_enum(
                    resource,
                    &[
                        "table",
                        "redis_key",
                        "kubernetes",
                        "sls_logstore",
                        "audit_events",
                    ],
                    &format!("Role {} rule {index} resource", role.metadata.name),
                )?;
                validate_enum(
                    &rule.effect,
                    &["allow", "deny"],
                    &format!("Role {} rule {index} effect", role.metadata.name),
                )?;
                if rule.resource_names.is_empty() {
                    return Err(format!(
                        "Role {} rule {index} resourceNames must not be empty",
                        role.metadata.name
                    ));
                }
                let mut seen = BTreeSet::new();
                for resource_name in &rule.resource_names {
                    if resource_name.is_empty() {
                        return Err(format!(
                            "Role {} rule {index} resourceNames contains an empty value",
                            role.metadata.name
                        ));
                    }
                    if !seen.insert(resource_name) {
                        return Err(format!(
                            "Role {} rule {index} has duplicate resourceName {}",
                            role.metadata.name, resource_name
                        ));
                    }
                }
            }
        }

        for binding in self.role_bindings.values() {
            validate_name(&binding.metadata.name, "RoleBinding")?;
            if !self.principals.contains_key(&binding.spec.principal) {
                return Err(format!(
                    "RoleBinding {} references missing Principal {}",
                    binding.metadata.name, binding.spec.principal
                ));
            }
            if !self.roles.contains_key(&binding.spec.role) {
                return Err(format!(
                    "RoleBinding {} references missing Role {}",
                    binding.metadata.name, binding.spec.role
                ));
            }
        }

        for api_key in self.api_keys.values() {
            validate_name(&api_key.metadata.name, "ApiKey")?;
            ensure_object(
                &api_key.spec.scopes,
                &format!("ApiKey {} spec.scopes", api_key.metadata.name),
            )?;
        }

        Ok(())
    }
}

pub fn build_gateway_config(
    context_name: String,
    endpoint: String,
    principal: String,
    token: String,
) -> GatewayConfig {
    GatewayConfig {
        api_version: API_VERSION.to_string(),
        kind: "GatewayConfig".to_string(),
        current_context: context_name.clone(),
        contexts: vec![GatewayContext {
            name: context_name,
            endpoint,
            principal,
        }],
        credentials: GatewayCredentials { token },
    }
}

pub fn is_agctl_policy(policy: &AccessPolicyRequest) -> bool {
    policy.constraints.get("managedBy").and_then(Value::as_str) == Some(MANAGED_BY)
}

pub fn policy_role_binding(policy: &AccessPolicyRequest) -> Option<&str> {
    policy
        .constraints
        .get("roleBinding")
        .and_then(Value::as_str)
}

fn default_active() -> String {
    "active".to_string()
}

fn default_allow() -> String {
    "allow".to_string()
}

fn empty_json_object() -> Value {
    json!({})
}

fn check_api_version(api_version: &str, kind: &str, name: &str) -> Result<(), String> {
    if api_version != API_VERSION {
        return Err(format!(
            "{kind} {name} has apiVersion {api_version}; expected {API_VERSION}"
        ));
    }
    Ok(())
}

fn insert_unique<T>(
    map: &mut BTreeMap<String, T>,
    name: String,
    value: T,
    kind: &str,
) -> Result<(), String> {
    if map.insert(name.clone(), value).is_some() {
        return Err(format!("duplicate {kind} manifest name {name}"));
    }
    Ok(())
}

fn validate_name(name: &str, kind: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{kind} metadata.name must not be empty"));
    }
    if name.len() > 96 {
        return Err(format!(
            "{kind} {name} metadata.name must be 96 bytes or fewer"
        ));
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(format!(
            "{kind} {name} metadata.name may contain only ASCII letters, numbers, '.', '-', and '_'"
        ));
    }
    Ok(())
}

fn validate_enum(value: &str, allowed: &[&str], label: &str) -> Result<(), String> {
    if allowed.contains(&value) {
        return Ok(());
    }
    Err(format!(
        "{label} must be one of {}; got {value}",
        allowed.join(", ")
    ))
}

fn validate_singleton(values: &[String], label: &str) -> Result<(), String> {
    if values.len() == 1 && !values[0].is_empty() {
        return Ok(());
    }
    Err(format!("{label} must contain exactly one non-empty value"))
}

fn ensure_object(value: &Value, label: &str) -> Result<(), String> {
    if value.is_object() {
        return Ok(());
    }
    Err(format!("{label} must be an object"))
}

fn normalize_optional_scope(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn empty_string_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn policy_id(binding_name: &str, rule_index: usize, resource_name: &str) -> Result<String, String> {
    let hash = short_hash(&format!("{binding_name}:{rule_index}:{resource_name}"));
    let id = format!("rb_{binding_name}_{rule_index}_{hash}");
    if id.len() > 128 {
        return Err(format!(
            "compiled policy id for RoleBinding {binding_name} exceeds 128 bytes"
        ));
    }
    Ok(id)
}

fn short_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut output = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
apiVersion: gateway.zenithinc.dev/v1
kind: Principal
metadata:
  name: svc-order-api
spec:
  type: service_account
---
apiVersion: gateway.zenithinc.dev/v1
kind: Role
metadata:
  name: order-db-reader
spec:
  scope:
    source: mysql-main
  rules:
    - tools: ["data.query_table"]
      verbs: ["select"]
      resources: ["table"]
      resourceNames: ["orders", "users", "payments"]
---
apiVersion: gateway.zenithinc.dev/v1
kind: RoleBinding
metadata:
  name: svc-order-api-order-db-reader
spec:
  principal: svc-order-api
  role: order-db-reader
"#;

    #[test]
    fn parses_all_supported_kinds() {
        let yaml = format!(
            "{SAMPLE}\n---\n{}",
            r#"
apiVersion: gateway.zenithinc.dev/v1
kind: ApiKey
metadata:
  name: svc-order-api-default
spec:
  principal: svc-order-api
  displayName: Default key
  scopes: {}
  expiresAt: null
"#
        );

        let manifests = ManifestSet::from_yaml_str(&yaml).unwrap();

        assert_eq!(manifests.principals.len(), 1);
        assert_eq!(manifests.roles.len(), 1);
        assert_eq!(manifests.role_bindings.len(), 1);
        assert_eq!(manifests.api_keys.len(), 1);
        assert_eq!(
            manifests.principals["svc-order-api"].spec.status,
            "active".to_string()
        );
    }

    #[test]
    fn accepts_legacy_project_environment_fields() {
        let yaml = r#"
apiVersion: gateway.zenithinc.dev/v1
kind: Principal
metadata:
  name: svc
spec:
  type: service_account
  defaultProject: legacy-project
  defaultEnvironment: legacy-env
---
apiVersion: gateway.zenithinc.dev/v1
kind: Role
metadata:
  name: reader
spec:
  scope:
    project: legacy-project
    environment: legacy-env
    source: mysql-main
  rules:
    - tools: ["data.query_table"]
      verbs: ["select"]
      resources: ["table"]
      resourceNames: ["orders"]
---
apiVersion: gateway.zenithinc.dev/v1
kind: RoleBinding
metadata:
  name: svc-reader
spec:
  principal: svc
  role: reader
"#;

        let manifests = ManifestSet::from_yaml_str(yaml).unwrap();
        let policies = manifests.compile_policies().unwrap();

        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].source_name.as_deref(), Some("mysql-main"));
    }

    #[test]
    fn rejects_missing_role_binding_principal() {
        let yaml = SAMPLE.replace("principal: svc-order-api", "principal: missing");

        let error = ManifestSet::from_yaml_str(&yaml).unwrap_err();

        assert!(error.contains("references missing Principal missing"));
    }

    #[test]
    fn rejects_missing_role_binding_role() {
        let yaml = SAMPLE.replace("role: order-db-reader", "role: missing-role");

        let error = ManifestSet::from_yaml_str(&yaml).unwrap_err();

        assert!(error.contains("references missing Role missing-role"));
    }

    #[test]
    fn rejects_duplicate_resource_names() {
        let yaml = SAMPLE.replace(
            "resourceNames: [\"orders\", \"users\", \"payments\"]",
            "resourceNames: [\"orders\", \"orders\"]",
        );

        let error = ManifestSet::from_yaml_str(&yaml).unwrap_err();

        assert!(error.contains("duplicate resourceName orders"));
    }

    #[test]
    fn rejects_invalid_resource_type() {
        let yaml = SAMPLE.replace("resources: [\"table\"]", "resources: [\"database\"]");

        let error = ManifestSet::from_yaml_str(&yaml).unwrap_err();

        assert!(error.contains("must be one of"));
    }

    #[test]
    fn rejects_empty_resource_names() {
        let yaml = SAMPLE.replace(
            "resourceNames: [\"orders\", \"users\", \"payments\"]",
            "resourceNames: []",
        );

        let error = ManifestSet::from_yaml_str(&yaml).unwrap_err();

        assert!(error.contains("resourceNames must not be empty"));
    }

    #[test]
    fn compiles_deterministic_policy_ids() {
        let manifests = ManifestSet::from_yaml_str(SAMPLE).unwrap();

        let first = manifests.compile_policies().unwrap();
        let second = manifests.compile_policies().unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first[0].id,
            "rb_svc-order-api-order-db-reader_0_3099584e2331a8a7"
        );
    }

    #[test]
    fn multi_table_role_creates_one_policy_per_table() {
        let manifests = ManifestSet::from_yaml_str(SAMPLE).unwrap();

        let policies = manifests.compile_policies().unwrap();

        assert_eq!(policies.len(), 3);
        assert_eq!(
            policies
                .iter()
                .map(|policy| policy.resource_pattern.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["orders", "users", "payments"]
        );
    }

    #[test]
    fn wildcard_resource_name_compiles() {
        let yaml = SAMPLE.replace(
            "resourceNames: [\"orders\", \"users\", \"payments\"]",
            "resourceNames: [\"*\"]",
        );
        let manifests = ManifestSet::from_yaml_str(&yaml).unwrap();

        let policies = manifests.compile_policies().unwrap();

        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].resource_pattern.as_deref(), Some("*"));
    }
}
