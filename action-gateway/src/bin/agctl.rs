use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use action_gateway_v2::rbac_manifest::{
    AccessPolicyRequest, ApiKeyRequest, ManifestSet, PrincipalRequest, build_gateway_config,
    is_agctl_policy, policy_role_binding,
};
use clap::{Args, Parser, Subcommand};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

type CliResult<T> = Result<T, String>;

#[derive(Debug, Parser)]
#[command(name = "agctl", about = "Declarative Action Gateway RBAC management")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Apply(ApplyArgs),
    Diff(DiffArgs),
    Delete(DeleteArgs),
    Create(CreateArgs),
    Get(GetArgs),
    Auth(AuthArgs),
}

#[derive(Debug, Args)]
struct ApplyArgs {
    #[arg(short = 'f', long = "file")]
    file: PathBuf,
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long = "admin-token")]
    admin_token: Option<String>,
    #[arg(long)]
    create_secrets: bool,
    #[arg(long)]
    prune: bool,
}

#[derive(Debug, Args)]
struct DiffArgs {
    #[arg(short = 'f', long = "file")]
    file: PathBuf,
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long = "admin-token")]
    admin_token: Option<String>,
    #[arg(long)]
    prune: bool,
}

#[derive(Debug, Args)]
struct DeleteArgs {
    #[arg(short = 'f', long = "file")]
    file: PathBuf,
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long = "admin-token")]
    admin_token: Option<String>,
    #[arg(long)]
    disable_principals: bool,
}

#[derive(Debug, Args)]
struct CreateArgs {
    #[command(subcommand)]
    command: CreateCommand,
}

#[derive(Debug, Subcommand)]
enum CreateCommand {
    #[command(name = "api-key")]
    ApiKey(CreateApiKeyArgs),
    Principal(CreatePrincipalArgs),
    User(CreateUserArgs),
}

#[derive(Debug, Args)]
struct CreateApiKeyArgs {
    principal: String,
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long = "admin-token")]
    admin_token: Option<String>,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, default_value = "prod")]
    context: String,
    #[arg(long = "expires-at")]
    expires_at: Option<String>,
    #[arg(long = "scopes-json", default_value = "{}")]
    scopes_json: String,
}

#[derive(Debug, Args)]
struct CreatePrincipalArgs {
    name: String,
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long = "admin-token")]
    admin_token: Option<String>,
    #[arg(long = "type", default_value = "service_account")]
    principal_type: String,
    #[arg(long)]
    display_name: Option<String>,
    #[arg(long, default_value = "active")]
    status: String,
    #[arg(long = "metadata-json", default_value = "{}")]
    metadata_json: String,
}

#[derive(Debug, Args)]
struct CreateUserArgs {
    name: String,
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long = "admin-token")]
    admin_token: Option<String>,
    #[arg(long)]
    display_name: Option<String>,
    #[arg(long, default_value = "active")]
    status: String,
    #[arg(long = "metadata-json", default_value = "{}")]
    metadata_json: String,
}

#[derive(Debug, Args)]
struct GetArgs {
    #[command(subcommand)]
    command: GetCommand,
}

#[derive(Debug, Subcommand)]
enum GetCommand {
    Principal(GetPrincipalArgs),
    Principals(GetPrincipalsArgs),
    Users(GetPrincipalsArgs),
}

#[derive(Debug, Args)]
struct GetPrincipalArgs {
    name: String,
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long = "admin-token")]
    admin_token: Option<String>,
}

#[derive(Debug, Args)]
struct GetPrincipalsArgs {
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long = "admin-token")]
    admin_token: Option<String>,
}

#[derive(Debug, Args)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    #[command(name = "can-i")]
    CanI(CanIArgs),
}

#[derive(Debug, Args)]
struct CanIArgs {
    #[arg(short = 'f', long = "file")]
    file: Option<PathBuf>,
    #[arg(long = "as")]
    principal: String,
    #[arg(long)]
    verb: String,
    #[arg(long)]
    resource: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    source: Option<String>,
    #[arg(long)]
    tool: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ItemsResponse<T> {
    items: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct ApiKeyResponse {
    id: String,
    #[serde(rename = "apiKey")]
    api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RemotePrincipal {
    id: String,
    #[serde(rename = "principalType")]
    principal_type: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    status: String,
    metadata: Value,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> CliResult<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Apply(args) => apply(args).await,
        Command::Diff(args) => diff(args).await,
        Command::Delete(args) => delete(args).await,
        Command::Create(args) => match args.command {
            CreateCommand::ApiKey(args) => create_api_key(args).await,
            CreateCommand::Principal(args) => create_principal(args).await,
            CreateCommand::User(args) => create_user(args).await,
        },
        Command::Get(args) => match args.command {
            GetCommand::Principal(args) => get_principal(args).await,
            GetCommand::Principals(args) => get_principals(args, None).await,
            GetCommand::Users(args) => get_principals(args, Some("user")).await,
        },
        Command::Auth(args) => match args.command {
            AuthCommand::CanI(args) => can_i(args),
        },
    }
}

async fn apply(args: ApplyArgs) -> CliResult<()> {
    let manifests = read_manifests(&args.file)?;
    let endpoint = required_endpoint(args.endpoint)?;
    let admin_token = required_admin_token(args.admin_token)?;
    let client = Client::new();
    let policies = manifests.compile_policies()?;

    for principal in manifests.principal_requests() {
        post_json::<_, Value>(&client, &endpoint, &admin_token, "/principals", &principal).await?;
        println!("upserted principal {}", principal.id);
    }

    if args.create_secrets {
        for api_key in manifests.api_key_requests() {
            let response: ApiKeyResponse =
                post_json(&client, &endpoint, &admin_token, "/api-keys", &api_key).await?;
            println!(
                "created api-key {} for {}",
                response.id, api_key.principal_id
            );
            println!("token: {}", response.api_key);
        }
    } else {
        for api_key in manifests.api_keys.keys() {
            println!("skipped api-key {api_key}; pass --create-secrets to create secrets");
        }
    }

    for policy in &policies {
        post_json::<_, Value>(&client, &endpoint, &admin_token, "/access-policies", policy).await?;
        println!("upserted access-policy {}", policy.id);
    }

    if args.prune {
        let stale =
            stale_managed_policies(&client, &endpoint, &admin_token, &manifests, &policies).await?;
        for policy in stale {
            let disabled = disabled_policy(policy);
            post_json::<_, Value>(
                &client,
                &endpoint,
                &admin_token,
                "/access-policies",
                &disabled,
            )
            .await?;
            println!("disabled stale access-policy {}", disabled.id);
        }
    }

    Ok(())
}

async fn diff(args: DiffArgs) -> CliResult<()> {
    let manifests = read_manifests(&args.file)?;
    let endpoint = required_endpoint(args.endpoint)?;
    let admin_token = required_admin_token(args.admin_token)?;
    let client = Client::new();
    let desired_principals = manifests.principal_requests();
    let desired_policies = manifests.compile_policies()?;
    let remote_principals = list_principals(&client, &endpoint, &admin_token).await?;
    let remote_policies = list_policies(&client, &endpoint, &admin_token).await?;
    let remote_principal_map = remote_principals
        .into_iter()
        .map(|principal| (principal.id.clone(), principal))
        .collect::<BTreeMap<_, _>>();
    let remote_policy_map = remote_policies
        .into_iter()
        .map(|policy| (policy.id.clone(), policy))
        .collect::<BTreeMap<_, _>>();
    let mut changes = 0;

    for principal in &desired_principals {
        match remote_principal_map.get(&principal.id) {
            None => {
                changes += 1;
                println!("+ principal {}", principal.id);
            }
            Some(remote) if principal_needs_update(remote, principal) => {
                changes += 1;
                println!("~ principal {}", principal.id);
            }
            Some(_) => {}
        }
    }

    for api_key in manifests.api_keys.keys() {
        changes += 1;
        println!("? api-key {api_key} is declarative; apply creates it only with --create-secrets");
    }

    for policy in &desired_policies {
        match remote_policy_map.get(&policy.id) {
            None => {
                changes += 1;
                println!("+ access-policy {}", policy.id);
            }
            Some(remote) if policy_needs_update(remote, policy) => {
                changes += 1;
                println!("~ access-policy {}", policy.id);
            }
            Some(_) => {}
        }
    }

    let desired_ids = desired_policy_ids(&desired_policies);
    let role_bindings = manifests.role_binding_names();
    for policy in remote_policy_map.values() {
        if is_stale_managed_policy(policy, &role_bindings, &desired_ids) {
            changes += 1;
            if args.prune {
                println!("- access-policy {} (disable)", policy.id);
            } else {
                println!(
                    "- access-policy {} (stale; apply --prune disables it)",
                    policy.id
                );
            }
        }
    }

    if changes == 0 {
        println!("no changes");
    }

    Ok(())
}

async fn delete(args: DeleteArgs) -> CliResult<()> {
    let manifests = read_manifests(&args.file)?;
    let endpoint = required_endpoint(args.endpoint)?;
    let admin_token = required_admin_token(args.admin_token)?;
    let client = Client::new();
    let role_bindings = manifests.role_binding_names();
    let policies = list_policies(&client, &endpoint, &admin_token).await?;
    let mut disabled_count = 0;

    for policy in policies {
        if is_agctl_policy(&policy)
            && policy_role_binding(&policy).is_some_and(|name| role_bindings.contains(name))
            && policy.enabled
        {
            let disabled = disabled_policy(policy);
            post_json::<_, Value>(
                &client,
                &endpoint,
                &admin_token,
                "/access-policies",
                &disabled,
            )
            .await?;
            disabled_count += 1;
            println!("disabled access-policy {}", disabled.id);
        }
    }

    if disabled_count == 0 {
        println!("no matching managed access-policies");
    }

    if args.disable_principals {
        for mut principal in manifests.principal_requests() {
            principal.status = "disabled".to_string();
            post_json::<_, Value>(&client, &endpoint, &admin_token, "/principals", &principal)
                .await?;
            println!("disabled principal {}", principal.id);
        }
    }

    Ok(())
}

async fn create_api_key(args: CreateApiKeyArgs) -> CliResult<()> {
    let endpoint = required_endpoint(args.endpoint)?;
    let admin_token = required_admin_token(args.admin_token)?;
    let scopes = parse_json_object(&args.scopes_json, "--scopes-json")?;
    let client = Client::new();
    let request = ApiKeyRequest {
        principal_id: args.principal.clone(),
        id: args.id.clone(),
        name: args.name.clone(),
        expires_at: args.expires_at.clone(),
        scopes,
    };

    let response: ApiKeyResponse =
        post_json(&client, &endpoint, &admin_token, "/api-keys", &request).await?;
    println!("created api-key {}", response.id);
    println!("token: {}", response.api_key);

    if let Some(out) = args.out {
        let config = build_gateway_config(
            args.context,
            mcp_endpoint(&endpoint),
            args.principal,
            response.api_key,
        );
        let yaml = serde_yaml::to_string(&config)
            .map_err(|error| format!("failed to render GatewayConfig YAML: {error}"))?;
        fs::write(&out, yaml)
            .map_err(|error| format!("failed to write {}: {error}", out.display()))?;
        println!("wrote {}", out.display());
    }

    Ok(())
}

async fn create_principal(args: CreatePrincipalArgs) -> CliResult<()> {
    let endpoint = required_endpoint(args.endpoint)?;
    let admin_token = required_admin_token(args.admin_token)?;
    let metadata = parse_json_object(&args.metadata_json, "--metadata-json")?;
    validate_principal_type(&args.principal_type)?;
    validate_principal_status(&args.status)?;
    let client = Client::new();
    let request = PrincipalRequest {
        id: args.name,
        principal_type: args.principal_type,
        display_name: args.display_name,
        status: args.status,
        metadata,
    };

    post_json::<_, Value>(&client, &endpoint, &admin_token, "/principals", &request).await?;
    println!("upserted principal {}", request.id);
    Ok(())
}

async fn create_user(args: CreateUserArgs) -> CliResult<()> {
    create_principal(CreatePrincipalArgs {
        name: args.name,
        endpoint: args.endpoint,
        admin_token: args.admin_token,
        principal_type: "user".to_string(),
        display_name: args.display_name,
        status: args.status,
        metadata_json: args.metadata_json,
    })
    .await
}

async fn get_principal(args: GetPrincipalArgs) -> CliResult<()> {
    let endpoint = required_endpoint(args.endpoint)?;
    let admin_token = required_admin_token(args.admin_token)?;
    let client = Client::new();
    let principals = list_principals(&client, &endpoint, &admin_token).await?;
    let principal = principals
        .into_iter()
        .find(|principal| principal.id == args.name)
        .ok_or_else(|| format!("principal {} not found", args.name))?;

    print_principal_detail(&principal)?;
    Ok(())
}

async fn get_principals(args: GetPrincipalsArgs, principal_type: Option<&str>) -> CliResult<()> {
    let endpoint = required_endpoint(args.endpoint)?;
    let admin_token = required_admin_token(args.admin_token)?;
    let client = Client::new();
    let mut principals = list_principals(&client, &endpoint, &admin_token).await?;
    if let Some(principal_type) = principal_type {
        principals.retain(|principal| principal.principal_type == principal_type);
    }

    print_principal_table(&principals);
    Ok(())
}

fn can_i(args: CanIArgs) -> CliResult<()> {
    let Some(file) = args.file else {
        return Err(
            "remote auth can-i is not implemented yet; pass -f for a local manifest check"
                .to_string(),
        );
    };
    let manifests = read_manifests(&file)?;
    let policies = manifests.compile_policies()?;
    let query = CanIQuery {
        principal: args.principal,
        verb: args.verb,
        resource: args.resource,
        name: args.name,
        source: args.source,
        tool: args.tool,
    };
    let decision = local_can_i(&policies, &query);

    if decision.allowed {
        println!("yes");
        if let Some(policy_id) = decision.policy_id {
            println!("matched access-policy {policy_id}");
        }
        Ok(())
    } else {
        println!("no");
        if let Some(policy_id) = decision.policy_id {
            println!("denied by access-policy {policy_id}");
        } else {
            println!("no matching allow policy");
        }
        std::process::exit(3);
    }
}

fn read_manifests(path: &Path) -> CliResult<ManifestSet> {
    let input = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    ManifestSet::from_yaml_str(&input)
}

async fn list_principals(
    client: &Client,
    endpoint: &str,
    admin_token: &str,
) -> CliResult<Vec<RemotePrincipal>> {
    let response: ItemsResponse<RemotePrincipal> =
        get_json(client, endpoint, admin_token, "/principals").await?;
    Ok(response.items)
}

async fn list_policies(
    client: &Client,
    endpoint: &str,
    admin_token: &str,
) -> CliResult<Vec<AccessPolicyRequest>> {
    let response: ItemsResponse<AccessPolicyRequest> =
        get_json(client, endpoint, admin_token, "/access-policies").await?;
    Ok(response.items)
}

async fn stale_managed_policies(
    client: &Client,
    endpoint: &str,
    admin_token: &str,
    manifests: &ManifestSet,
    desired_policies: &[AccessPolicyRequest],
) -> CliResult<Vec<AccessPolicyRequest>> {
    let desired_ids = desired_policy_ids(desired_policies);
    let role_bindings = manifests.role_binding_names();
    let existing = list_policies(client, endpoint, admin_token).await?;
    Ok(existing
        .into_iter()
        .filter(|policy| is_stale_managed_policy(policy, &role_bindings, &desired_ids))
        .collect())
}

fn desired_policy_ids(policies: &[AccessPolicyRequest]) -> BTreeSet<String> {
    policies.iter().map(|policy| policy.id.clone()).collect()
}

fn is_stale_managed_policy(
    policy: &AccessPolicyRequest,
    role_bindings: &BTreeSet<String>,
    desired_ids: &BTreeSet<String>,
) -> bool {
    is_agctl_policy(policy)
        && policy.enabled
        && !desired_ids.contains(&policy.id)
        && policy_role_binding(policy).is_some_and(|name| role_bindings.contains(name))
}

fn disabled_policy(mut policy: AccessPolicyRequest) -> AccessPolicyRequest {
    policy.enabled = false;
    policy
}

async fn get_json<T: DeserializeOwned>(
    client: &Client,
    endpoint: &str,
    admin_token: &str,
    path: &str,
) -> CliResult<T> {
    let response = client
        .get(admin_url(endpoint, path))
        .bearer_auth(admin_token)
        .send()
        .await
        .map_err(|error| format!("HTTP GET {path} failed: {error}"))?;
    decode_response(response).await
}

async fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(
    client: &Client,
    endpoint: &str,
    admin_token: &str,
    path: &str,
    body: &B,
) -> CliResult<T> {
    let response = client
        .post(admin_url(endpoint, path))
        .bearer_auth(admin_token)
        .json(body)
        .send()
        .await
        .map_err(|error| format!("HTTP POST {path} failed: {error}"))?;
    decode_response(response).await
}

async fn decode_response<T: DeserializeOwned>(response: reqwest::Response) -> CliResult<T> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("failed to read HTTP response: {error}"))?;
    if !status.is_success() {
        return Err(format_http_error(status, &text));
    }
    serde_json::from_str(&text).map_err(|error| format!("failed to decode HTTP response: {error}"))
}

fn format_http_error(status: StatusCode, body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return format!("gateway returned {status}");
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            return format!("gateway returned {status}: {message}");
        }
    }
    format!("gateway returned {status}: {trimmed}")
}

fn required_endpoint(endpoint: Option<String>) -> CliResult<String> {
    endpoint
        .or_else(|| std::env::var("GATEWAY_ENDPOINT").ok())
        .map(|endpoint| endpoint.trim_end_matches('/').to_string())
        .filter(|endpoint| !endpoint.is_empty())
        .ok_or_else(|| "missing --endpoint or GATEWAY_ENDPOINT".to_string())
}

fn required_admin_token(admin_token: Option<String>) -> CliResult<String> {
    admin_token
        .or_else(|| std::env::var("GATEWAY_ADMIN_TOKEN").ok())
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| "missing --admin-token or GATEWAY_ADMIN_TOKEN".to_string())
}

fn admin_url(endpoint: &str, path: &str) -> String {
    format!(
        "{}/admin/{}",
        gateway_base(endpoint),
        path.trim_start_matches('/')
    )
}

fn mcp_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    if trimmed.ends_with("/mcp") || trimmed.ends_with("/rpc") {
        trimmed.to_string()
    } else {
        format!("{}/mcp", gateway_base(trimmed))
    }
}

fn gateway_base(endpoint: &str) -> String {
    let mut base = endpoint.trim_end_matches('/').to_string();
    for suffix in ["/mcp", "/rpc", "/admin"] {
        if base.ends_with(suffix) {
            base.truncate(base.len() - suffix.len());
            break;
        }
    }
    base
}

fn principal_needs_update(remote: &RemotePrincipal, desired: &PrincipalRequest) -> bool {
    remote.principal_type != desired.principal_type
        || remote.display_name != desired.display_name
        || remote.status != desired.status
        || remote.metadata != desired.metadata
}

fn policy_needs_update(remote: &AccessPolicyRequest, desired: &AccessPolicyRequest) -> bool {
    remote.principal_id != desired.principal_id
        || remote.api_key_id != desired.api_key_id
        || remote.effect != desired.effect
        || remote.source_name != desired.source_name
        || remote.tool_name != desired.tool_name
        || remote.action_name != desired.action_name
        || remote.resource_type != desired.resource_type
        || remote.resource_pattern != desired.resource_pattern
        || remote.constraints != desired.constraints
        || remote.specificity != desired.specificity
        || remote.enabled != desired.enabled
}

fn parse_json_object(input: &str, label: &str) -> CliResult<Value> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| format!("{label} must be valid JSON: {error}"))?;
    if !value.is_object() {
        return Err(format!("{label} must be a JSON object"));
    }
    Ok(value)
}

fn validate_principal_type(value: &str) -> CliResult<()> {
    if ["user", "service_account", "legacy_admin"].contains(&value) {
        return Ok(());
    }
    Err(format!(
        "--type must be one of user, service_account, legacy_admin; got {value}"
    ))
}

fn validate_principal_status(value: &str) -> CliResult<()> {
    if ["active", "disabled"].contains(&value) {
        return Ok(());
    }
    Err(format!("--status must be active or disabled; got {value}"))
}

fn print_principal_table(principals: &[RemotePrincipal]) {
    println!("ID\tTYPE\tSTATUS\tDISPLAY_NAME");
    for principal in principals {
        println!(
            "{}\t{}\t{}\t{}",
            principal.id,
            principal.principal_type,
            principal.status,
            principal.display_name.as_deref().unwrap_or("-")
        );
    }
}

fn print_principal_detail(principal: &RemotePrincipal) -> CliResult<()> {
    println!("id: {}", principal.id);
    println!("type: {}", principal.principal_type);
    println!("status: {}", principal.status);
    println!(
        "displayName: {}",
        principal.display_name.as_deref().unwrap_or("-")
    );
    let metadata = serde_json::to_string_pretty(&principal.metadata)
        .map_err(|error| format!("failed to format principal metadata: {error}"))?;
    println!("metadata: {metadata}");
    Ok(())
}

#[derive(Debug)]
struct CanIQuery {
    principal: String,
    verb: String,
    resource: String,
    name: String,
    source: Option<String>,
    tool: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct CanIDecision {
    allowed: bool,
    policy_id: Option<String>,
}

fn local_can_i(policies: &[AccessPolicyRequest], query: &CanIQuery) -> CanIDecision {
    let inferred_tool = query
        .tool
        .clone()
        .or_else(|| infer_tool_name(&query.resource, &query.verb));

    for policy in policies {
        if policy.effect == "deny" && policy_matches_query(policy, query, inferred_tool.as_deref())
        {
            return CanIDecision {
                allowed: false,
                policy_id: Some(policy.id.clone()),
            };
        }
    }

    for policy in policies {
        if policy.effect == "allow" && policy_matches_query(policy, query, inferred_tool.as_deref())
        {
            return CanIDecision {
                allowed: true,
                policy_id: Some(policy.id.clone()),
            };
        }
    }

    CanIDecision {
        allowed: false,
        policy_id: None,
    }
}

fn policy_matches_query(
    policy: &AccessPolicyRequest,
    query: &CanIQuery,
    inferred_tool: Option<&str>,
) -> bool {
    policy.enabled
        && policy.principal_id.as_deref() == Some(query.principal.as_str())
        && optional_requested_match(policy.tool_name.as_deref(), inferred_tool)
        && optional_requested_match(policy.action_name.as_deref(), Some(query.verb.as_str()))
        && optional_requested_match(
            policy.resource_type.as_deref(),
            Some(query.resource.as_str()),
        )
        && resource_pattern_matches(policy.resource_pattern.as_deref(), &query.name)
        && optional_requested_match(policy.source_name.as_deref(), query.source.as_deref())
}

fn optional_requested_match(policy_value: Option<&str>, requested: Option<&str>) -> bool {
    match requested {
        Some(requested) => policy_value.is_none_or(|policy_value| policy_value == requested),
        None => true,
    }
}

fn resource_pattern_matches(pattern: Option<&str>, value: &str) -> bool {
    let Some(pattern) = pattern else {
        return true;
    };
    if pattern == "*" {
        return true;
    }
    wildcard_match(pattern, value)
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

fn infer_tool_name(resource: &str, verb: &str) -> Option<String> {
    match (resource, verb) {
        ("table", "select") => Some("data.query_table".to_string()),
        ("redis_key", "get") => Some("redis.query_key".to_string()),
        ("kubernetes", "list") => Some("kubernetes.list_resources".to_string()),
        ("kubernetes", "get") => Some("kubernetes.get_resource".to_string()),
        ("kubernetes", "rollout_status" | "rollout_history") => {
            Some("kubernetes.rollout_status".to_string())
        }
        ("kubernetes", "logs") => Some("kubernetes.query_pod_logs".to_string()),
        ("kubernetes", "raw_read") => Some("kubernetes.kubectl_read".to_string()),
        ("sls_logstore", "query") => Some("logs.query_sls_logs".to_string()),
        ("audit_events", "query") => Some("audit.query_approval_events".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_admin_urls_from_gateway_endpoints() {
        assert_eq!(
            admin_url("http://127.0.0.1:8081", "/access-policies"),
            "http://127.0.0.1:8081/admin/access-policies"
        );
        assert_eq!(
            admin_url("http://127.0.0.1:8081/mcp", "principals"),
            "http://127.0.0.1:8081/admin/principals"
        );
        assert_eq!(
            mcp_endpoint("http://127.0.0.1:8081"),
            "http://127.0.0.1:8081/mcp"
        );
    }

    #[test]
    fn local_can_i_allows_compiled_policy() {
        let policy = AccessPolicyRequest {
            id: "rb_test_0_hash".to_string(),
            principal_id: Some("svc".to_string()),
            api_key_id: None,
            effect: "allow".to_string(),
            source_name: Some("mysql-main".to_string()),
            tool_name: Some("data.query_table".to_string()),
            action_name: Some("select".to_string()),
            resource_type: Some("table".to_string()),
            resource_pattern: Some("orders".to_string()),
            constraints: serde_json::json!({ "managedBy": "agctl" }),
            specificity: 0,
            enabled: true,
        };
        let query = CanIQuery {
            principal: "svc".to_string(),
            verb: "select".to_string(),
            resource: "table".to_string(),
            name: "orders".to_string(),
            source: Some("mysql-main".to_string()),
            tool: None,
        };

        let decision = local_can_i(&[policy], &query);

        assert!(decision.allowed);
        assert_eq!(decision.policy_id.as_deref(), Some("rb_test_0_hash"));
    }
}
