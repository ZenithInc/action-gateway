use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use action_gateway_v2::sls::{GetLogsV2Request, SlsClient, SlsCredentials};
use clap::Parser;
use serde_json::{Value, json};

#[derive(Debug, Parser)]
#[command(
    name = "sls-check",
    about = "Validate Alibaba Cloud SLS credentials and the Gateway GetLogsV2 client"
)]
struct Args {
    #[arg(long, default_value = ".env")]
    env_file: PathBuf,

    #[arg(long)]
    endpoint: Option<String>,

    #[arg(long)]
    project: Option<String>,

    #[arg(long)]
    logstore: Option<String>,

    #[arg(long, default_value = "*")]
    query: String,

    #[arg(long)]
    from: Option<u64>,

    #[arg(long)]
    to: Option<u64>,

    #[arg(long, default_value_t = 1)]
    line: usize,

    #[arg(long, default_value_t = 0)]
    offset: usize,

    #[arg(long, default_value_t = true)]
    reverse: bool,

    #[arg(long)]
    topic: Option<String>,

    #[arg(long)]
    power_sql: bool,

    #[arg(long)]
    show_logs: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    match run(args).await {
        Ok(summary) => {
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        }
        Err(error) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": "failed",
                    "message": error.to_string()
                }))
                .unwrap()
            );
            std::process::exit(1);
        }
    }
}

async fn run(args: Args) -> Result<Value, Box<dyn Error>> {
    let env = load_env_file(&args.env_file)?;
    let endpoint = args
        .endpoint
        .or_else(|| lookup(&env, &["SLS_ENDPOINT", "ALIYUN_LOG_ENDPOINT", "ENDPOINT"]))
        .ok_or("missing SLS endpoint; pass --endpoint or set SLS_ENDPOINT in .env")?;
    let project = args
        .project
        .or_else(|| lookup(&env, &["SLS_PROJECT", "PROJECT", "PROJECT_NAME"]))
        .ok_or("missing SLS project; pass --project or set SLS_PROJECT in .env")?;
    let logstore = args
        .logstore
        .or_else(|| lookup(&env, &["SLS_LOGSTORE", "LOGSTORE", "LOGSTORE_NAME"]))
        .ok_or("missing SLS logstore; pass --logstore or set SLS_LOGSTORE in .env")?;
    let access_key_id = lookup(
        &env,
        &[
            "AccessKeyID",
            "AccessKeyId",
            "ACCESS_KEY_ID",
            "ALIYUN_ACCESS_KEY_ID",
            "ALIBABA_CLOUD_ACCESS_KEY_ID",
        ],
    )
    .ok_or("missing AccessKey ID in .env")?;
    let access_key_secret = lookup(
        &env,
        &[
            "AccessKeySecret",
            "ACCESS_KEY_SECRET",
            "ALIYUN_ACCESS_KEY_SECRET",
            "ALIBABA_CLOUD_ACCESS_KEY_SECRET",
        ],
    )
    .ok_or("missing AccessKey Secret in .env")?;
    let security_token = lookup(
        &env,
        &[
            "SecurityToken",
            "SECURITY_TOKEN",
            "SLS_SECURITY_TOKEN",
            "ALIYUN_SECURITY_TOKEN",
            "ALIBABA_CLOUD_SECURITY_TOKEN",
        ],
    );
    let (from, to) = time_range(args.from, args.to)?;

    let request = GetLogsV2Request {
        project: project.clone(),
        logstore: logstore.clone(),
        from,
        to,
        query: args.query,
        line: args.line,
        offset: args.offset,
        reverse: args.reverse,
        topic: args.topic.clone(),
        power_sql: args.power_sql,
    };
    validate_request(&request)?;

    let client = SlsClient::new(
        &endpoint,
        SlsCredentials {
            access_key_id: access_key_id.clone(),
            access_key_secret,
            security_token: security_token.clone(),
        },
    )?;
    let response = client.get_logs_v2(&request).await?;
    let log_count = response.logs.len();
    let first_log_keys = response
        .logs
        .first()
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut summary = json!({
        "status": "succeeded",
        "credential": {
            "accessKeyIdPresent": true,
            "accessKeyIdSuffix": suffix(&access_key_id, 4),
            "accessKeySecretPresent": true,
            "securityTokenPresent": security_token.is_some()
        },
        "request": {
            "endpoint": endpoint,
            "project": project,
            "logstore": logstore,
            "from": request.from,
            "to": request.to,
            "line": request.line,
            "offset": request.offset,
            "reverse": request.reverse,
            "topic": request.topic,
            "powerSql": request.power_sql,
            "queryPresent": true
        },
        "response": {
            "progress": response.progress,
            "count": response.count,
            "logCount": log_count,
            "firstLogKeys": first_log_keys
        }
    });
    if args.show_logs {
        summary["response"]["logs"] = Value::Array(response.logs);
    }

    Ok(summary)
}

fn load_env_file(path: &PathBuf) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut values = BTreeMap::new();
    for (line_number, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = split_env_line(line) else {
            return Err(format!(
                "invalid .env line {}: expected KEY=value or KEY: value",
                line_number + 1
            )
            .into());
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("invalid .env line {}: empty key", line_number + 1).into());
        }
        values.insert(key.to_string(), unquote(value.trim()).to_string());
    }

    Ok(values)
}

fn split_env_line(line: &str) -> Option<(&str, &str)> {
    line.split_once('=').or_else(|| line.split_once(':'))
}

fn unquote(value: &str) -> &str {
    if value.len() >= 2 {
        if let Some(value) = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            return value;
        }
        if let Some(value) = value
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
        {
            return value;
        }
    }

    value
}

fn lookup(values: &BTreeMap<String, String>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = values.get(*key).filter(|value| !value.trim().is_empty()) {
            return Some(value.trim().to_string());
        }
        if let Ok(value) = std::env::var(key)
            && !value.trim().is_empty()
        {
            return Some(value.trim().to_string());
        }
    }

    None
}

fn time_range(from: Option<u64>, to: Option<u64>) -> Result<(u64, u64), Box<dyn Error>> {
    match (from, to) {
        (Some(from), Some(to)) if from < to => Ok((from, to)),
        (Some(_), Some(_)) => Err("from must be less than to".into()),
        (None, None) => {
            let to = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            Ok((to.saturating_sub(900), to))
        }
        _ => Err("from and to must be provided together".into()),
    }
}

fn validate_request(request: &GetLogsV2Request) -> Result<(), Box<dyn Error>> {
    if request.project.trim().is_empty() {
        return Err("project must not be empty".into());
    }
    if request.logstore.trim().is_empty() {
        return Err("logstore must not be empty".into());
    }
    if request.query.trim().is_empty() {
        return Err("query must not be empty".into());
    }
    if request.line > 100 {
        return Err("line must be between 0 and 100".into());
    }

    Ok(())
}

fn suffix(value: &str, count: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    chars
        .iter()
        .skip(chars.len().saturating_sub(count))
        .collect()
}
