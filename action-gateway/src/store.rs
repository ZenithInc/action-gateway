use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::rbac_manifest::{AccessPolicyRequest, ApiKeyRequest, PrincipalRequest};

#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
    state: Arc<RwLock<GatewayState>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayState {
    #[serde(default)]
    pub principals: Vec<PrincipalRecord>,
    #[serde(default)]
    pub api_keys: Vec<ApiKeyRecord>,
    #[serde(default)]
    pub access_policies: Vec<AccessPolicyRecord>,
    #[serde(default)]
    pub sources: Vec<SourceRecord>,
    #[serde(default)]
    pub table_allowlist: Vec<TableAllowlistRecord>,
    #[serde(default)]
    pub redis_key_allowlist: Vec<RedisKeyAllowlistRecord>,
    #[serde(default)]
    pub kubernetes_resource_allowlist: Vec<KubernetesResourceAllowlistRecord>,
    #[serde(default)]
    pub audit_events: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrincipalRecord {
    pub id: String,
    pub principal_type: String,
    pub display_name: Option<String>,
    pub status: String,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyRecord {
    pub id: String,
    pub principal_id: String,
    pub name: Option<String>,
    pub secret_salt: String,
    pub secret_hash: String,
    pub scopes: Value,
    pub expires_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessPolicyRecord {
    pub id: String,
    pub principal_id: Option<String>,
    pub api_key_id: Option<String>,
    pub effect: String,
    pub source_name: Option<String>,
    pub tool_name: Option<String>,
    pub action_name: Option<String>,
    pub resource_type: Option<String>,
    pub resource_pattern: Option<String>,
    pub constraints: Value,
    pub specificity: i64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRecord {
    pub id: String,
    pub source_name: String,
    pub source_type: String,
    pub display_name: Option<String>,
    pub config: Value,
    pub credential: Option<Value>,
    pub credential_version: Option<i64>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableAllowlistRecord {
    pub source_name: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub max_limit: i64,
    pub max_estimated_rows: i64,
    pub mask_rules: Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyAllowlistRecord {
    pub source_name: String,
    pub key_pattern: String,
    pub max_value_bytes: usize,
    pub max_members: usize,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesResourceAllowlistRecord {
    pub source_name: String,
    pub namespace: String,
    pub resource: String,
    pub actions: Vec<String>,
    pub max_items: usize,
    pub max_output_bytes: usize,
    pub max_tail_lines: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct AuthApiKeyRecord {
    pub api_key_id: String,
    pub principal_id: String,
    pub principal_type: String,
    pub principal_name: Option<String>,
    pub key_status: String,
    pub principal_status: String,
    pub scopes: Value,
    pub expired: bool,
}

impl FileStore {
    pub async fn load(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let state = if path.exists() {
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            if bytes.is_empty() {
                GatewayState::default()
            } else {
                serde_json::from_slice::<GatewayState>(&bytes)
                    .map_err(|error| format!("failed to parse {}: {error}", path.display()))?
            }
        } else {
            GatewayState::default()
        };
        let store = Self {
            path,
            state: Arc::new(RwLock::new(state)),
        };
        store.persist().await?;
        Ok(store)
    }

    pub async fn authenticate_api_key(
        &self,
        api_key_id: &str,
        secret: &str,
    ) -> Result<Option<AuthApiKeyRecord>, String> {
        let mut state = self.state.write().await;
        let Some(key_index) = state.api_keys.iter().position(|key| {
            key.id == api_key_id && key.secret_hash == hash_secret(&key.secret_salt, secret)
        }) else {
            return Ok(None);
        };
        let key = state.api_keys[key_index].clone();
        let Some(principal) = state
            .principals
            .iter()
            .find(|principal| principal.id == key.principal_id)
            .cloned()
        else {
            return Ok(None);
        };
        state.api_keys[key_index].last_used_at = Some(now_string());
        let row = AuthApiKeyRecord {
            api_key_id: key.id,
            principal_id: principal.id,
            principal_type: principal.principal_type,
            principal_name: principal.display_name,
            key_status: key.status,
            principal_status: principal.status,
            scopes: key.scopes,
            expired: key.expires_at.as_deref().is_some_and(is_expired),
        };
        drop(state);
        self.persist().await?;
        Ok(Some(row))
    }

    pub async fn list_principals(&self) -> Vec<PrincipalRecord> {
        self.state.read().await.principals.clone()
    }

    pub async fn upsert_principal(
        &self,
        request: PrincipalRequest,
    ) -> Result<PrincipalRecord, String> {
        validate_principal_type(&request.principal_type)?;
        validate_principal_status(&request.status)?;
        let now = now_string();
        let mut state = self.state.write().await;
        let record = PrincipalRecord {
            id: request.id,
            principal_type: request.principal_type,
            display_name: request.display_name,
            status: request.status,
            metadata: request.metadata,
            created_at: now.clone(),
            updated_at: now,
        };
        if let Some(existing) = state
            .principals
            .iter_mut()
            .find(|principal| principal.id == record.id)
        {
            let created_at = existing.created_at.clone();
            *existing = PrincipalRecord {
                created_at,
                ..record.clone()
            };
        } else {
            state.principals.push(record.clone());
        }
        drop(state);
        self.persist().await?;
        Ok(record)
    }

    pub async fn create_api_key(
        &self,
        request: ApiKeyRequest,
    ) -> Result<(ApiKeyRecord, String), String> {
        let mut state = self.state.write().await;
        if !state
            .principals
            .iter()
            .any(|principal| principal.id == request.principal_id && principal.status == "active")
        {
            return Err(format!(
                "principal {} was not found or is disabled",
                request.principal_id
            ));
        }
        let key_id = request
            .id
            .clone()
            .unwrap_or_else(|| format!("key_{}", Uuid::new_v4().simple()));
        if state.api_keys.iter().any(|key| key.id == key_id) {
            return Err(format!("api key {key_id} already exists"));
        }
        let secret = Uuid::new_v4().simple().to_string();
        let salt = Uuid::new_v4().simple().to_string();
        let now = now_string();
        let record = ApiKeyRecord {
            id: key_id,
            principal_id: request.principal_id,
            name: request.name,
            secret_salt: salt.clone(),
            secret_hash: hash_secret(&salt, &secret),
            scopes: request.scopes,
            expires_at: request.expires_at,
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
        };
        state.api_keys.push(record.clone());
        drop(state);
        self.persist().await?;
        Ok((record.clone(), format!("agk_{}_{}", record.id, secret)))
    }

    pub async fn list_access_policies(&self) -> Vec<AccessPolicyRecord> {
        self.state.read().await.access_policies.clone()
    }

    pub async fn upsert_access_policy(
        &self,
        request: AccessPolicyRequest,
    ) -> Result<AccessPolicyRecord, String> {
        if request.principal_id.is_none() && request.api_key_id.is_none() {
            return Err("principalId or apiKeyId is required".to_string());
        }
        if !["allow", "deny"].contains(&request.effect.as_str()) {
            return Err("effect must be allow or deny".to_string());
        }
        let now = now_string();
        let record = AccessPolicyRecord {
            id: request.id,
            principal_id: request.principal_id,
            api_key_id: request.api_key_id,
            effect: request.effect,
            source_name: request.source_name,
            tool_name: request.tool_name,
            action_name: request.action_name,
            resource_type: request.resource_type,
            resource_pattern: request.resource_pattern,
            constraints: request.constraints,
            specificity: request.specificity,
            enabled: request.enabled,
            created_at: now.clone(),
            updated_at: now,
        };
        let mut state = self.state.write().await;
        if let Some(existing) = state
            .access_policies
            .iter_mut()
            .find(|policy| policy.id == record.id)
        {
            let created_at = existing.created_at.clone();
            *existing = AccessPolicyRecord {
                created_at,
                ..record.clone()
            };
        } else {
            state.access_policies.push(record.clone());
        }
        drop(state);
        self.persist().await?;
        Ok(record)
    }

    pub async fn list_enabled_policies_for_auth(
        &self,
        principal_id: &str,
        api_key_id: Option<&str>,
    ) -> Vec<AccessPolicyRecord> {
        let mut rows = self
            .state
            .read()
            .await
            .access_policies
            .iter()
            .filter(|policy| {
                policy.enabled
                    && (policy.principal_id.as_deref() == Some(principal_id)
                        || api_key_id.is_some_and(|api_key_id| {
                            policy.api_key_id.as_deref() == Some(api_key_id)
                        }))
            })
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            let left_effect = if left.effect == "deny" { 0 } else { 1 };
            let right_effect = if right.effect == "deny" { 0 } else { 1 };
            left_effect
                .cmp(&right_effect)
                .then_with(|| right.specificity.cmp(&left.specificity))
                .then_with(|| left.created_at.cmp(&right.created_at))
        });
        rows
    }

    pub async fn allowed_tool_names(
        &self,
        principal_id: &str,
        api_key_id: Option<&str>,
    ) -> Option<BTreeSet<String>> {
        let rows = self
            .list_enabled_policies_for_auth(principal_id, api_key_id)
            .await
            .into_iter()
            .filter(|policy| policy.effect == "allow")
            .collect::<Vec<_>>();
        let mut names = BTreeSet::new();
        for policy in rows {
            let Some(tool_name) = policy.tool_name else {
                return None;
            };
            names.insert(tool_name);
        }
        Some(names)
    }

    pub async fn upsert_source(&self, mut record: SourceRecord) -> Result<SourceRecord, String> {
        let now = now_string();
        if record.id.is_empty() {
            record.id = format!("src_{}_{}", record.source_name, record.source_type);
        }
        if record.credential.is_some() && record.credential_version.is_none() {
            record.credential_version = Some(1);
        }
        record.created_at = now.clone();
        record.updated_at = now;
        let mut state = self.state.write().await;
        if let Some(existing) = state
            .sources
            .iter_mut()
            .find(|source| source.id == record.id)
        {
            let created_at = existing.created_at.clone();
            *existing = SourceRecord {
                created_at,
                ..record.clone()
            };
        } else {
            state.sources.push(record.clone());
        }
        drop(state);
        self.persist().await?;
        Ok(record)
    }

    pub async fn source(&self, source_name: &str, source_type: &str) -> Option<SourceRecord> {
        self.state
            .read()
            .await
            .sources
            .iter()
            .find(|source| {
                source.enabled
                    && source.source_name == source_name
                    && source.source_type == source_type
            })
            .cloned()
    }

    pub async fn table_allowlist(
        &self,
        source_name: &str,
        table_name: &str,
    ) -> Option<TableAllowlistRecord> {
        self.state
            .read()
            .await
            .table_allowlist
            .iter()
            .find(|record| {
                record.enabled
                    && record.source_name == source_name
                    && record.table_name == table_name
            })
            .cloned()
    }

    pub async fn redis_key_allowlist(&self, source_name: &str) -> Vec<RedisKeyAllowlistRecord> {
        self.state
            .read()
            .await
            .redis_key_allowlist
            .iter()
            .filter(|record| record.enabled && record.source_name == source_name)
            .cloned()
            .collect()
    }

    pub async fn kubernetes_policy(
        &self,
        source_name: &str,
        namespace: &str,
        resource: &str,
    ) -> Option<KubernetesResourceAllowlistRecord> {
        self.state
            .read()
            .await
            .kubernetes_resource_allowlist
            .iter()
            .find(|record| {
                record.enabled
                    && record.source_name == source_name
                    && record.namespace == namespace
                    && record.resource == resource
            })
            .cloned()
    }

    pub async fn append_audit_event(&self, event: Value) -> Result<(), String> {
        let mut state = self.state.write().await;
        state.audit_events.push(event);
        let overflow = state.audit_events.len().saturating_sub(10_000);
        if overflow > 0 {
            state.audit_events.drain(0..overflow);
        }
        drop(state);
        self.persist().await
    }

    pub async fn audit_events(&self) -> Vec<Value> {
        self.state.read().await.audit_events.clone()
    }

    async fn persist(&self) -> Result<(), String> {
        let state = self.state.read().await.clone();
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        let tmp = tmp_path(&self.path);
        let bytes = serde_json::to_vec_pretty(&state)
            .map_err(|error| format!("failed to serialize file store: {error}"))?;
        tokio::fs::write(&tmp, bytes)
            .await
            .map_err(|error| format!("failed to write {}: {error}", tmp.display()))?;
        tokio::fs::rename(&tmp, &self.path)
            .await
            .map_err(|error| format!("failed to replace {}: {error}", self.path.display()))
    }
}

impl AccessPolicyRecord {
    pub fn to_request(&self) -> AccessPolicyRequest {
        AccessPolicyRequest {
            id: self.id.clone(),
            principal_id: self.principal_id.clone(),
            api_key_id: self.api_key_id.clone(),
            effect: self.effect.clone(),
            source_name: self.source_name.clone(),
            tool_name: self.tool_name.clone(),
            action_name: self.action_name.clone(),
            resource_type: self.resource_type.clone(),
            resource_pattern: self.resource_pattern.clone(),
            constraints: self.constraints.clone(),
            specificity: self.specificity,
            enabled: self.enabled,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "principalId": self.principal_id,
            "apiKeyId": self.api_key_id,
            "effect": self.effect,
            "sourceName": self.source_name,
            "toolName": self.tool_name,
            "actionName": self.action_name,
            "resourceType": self.resource_type,
            "resourcePattern": self.resource_pattern,
            "constraints": self.constraints,
            "specificity": self.specificity,
            "enabled": self.enabled,
            "createdAt": self.created_at,
            "updatedAt": self.updated_at
        })
    }
}

impl PrincipalRecord {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "principalType": self.principal_type,
            "displayName": self.display_name,
            "status": self.status,
            "metadata": self.metadata,
            "createdAt": self.created_at,
            "updatedAt": self.updated_at
        })
    }
}

impl ApiKeyRecord {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "principalId": self.principal_id,
            "name": self.name,
            "status": self.status,
            "scopes": self.scopes,
            "expiresAt": self.expires_at,
            "createdAt": self.created_at,
            "lastUsedAt": self.last_used_at,
            "revokedAt": self.revoked_at
        })
    }
}

impl SourceRecord {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "sourceName": self.source_name,
            "sourceType": self.source_type,
            "displayName": self.display_name,
            "config": self.config,
            "activeCredentialVersion": self.credential_version,
            "enabled": self.enabled,
            "createdAt": self.created_at,
            "updatedAt": self.updated_at
        })
    }
}

pub fn hash_secret(salt: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(secret.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn now_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    seconds.to_string()
}

fn tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("gateway-store.json");
    path.with_file_name(format!("{file_name}.tmp"))
}

fn validate_principal_type(value: &str) -> Result<(), String> {
    if ["user", "service_account", "legacy_admin"].contains(&value) {
        return Ok(());
    }
    Err(format!(
        "principalType must be one of user, service_account, legacy_admin; got {value}"
    ))
}

fn validate_principal_status(value: &str) -> Result<(), String> {
    if ["active", "disabled"].contains(&value) {
        return Ok(());
    }
    Err(format!("status must be active or disabled; got {value}"))
}

fn is_expired(value: &str) -> bool {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|expires_at| expires_at <= OffsetDateTime::now_utc())
        .unwrap_or(false)
}

fn default_true() -> bool {
    true
}
