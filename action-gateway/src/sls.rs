use std::{collections::BTreeMap, io::Read, net::IpAddr, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use flate2::read::{GzDecoder, ZlibDecoder};
use hmac::{Hmac, Mac};
use md5::{Digest as _, Md5};
use reqwest::{
    Client, Url,
    header::{self, HeaderMap, HeaderName, HeaderValue},
};
use serde_json::{Value, json};
use sha1::Sha1;
use time::{OffsetDateTime, format_description};

type HmacSha1 = Hmac<Sha1>;

const SLS_API_VERSION: &str = "0.6.0";
const SLS_SIGNATURE_METHOD: &str = "hmac-sha1";
const SLS_CONTENT_TYPE: &str = "application/json";
const SLS_ACCEPT_ENCODING: &str = "gzip";
const SLS_REQUEST_TIMEOUT_SECONDS: u64 = 30;

#[derive(Debug, Clone)]
pub struct SlsCredentials {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub security_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SlsClient {
    endpoint: String,
    credentials: SlsCredentials,
    http: Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetLogsV2Request {
    pub project: String,
    pub logstore: String,
    pub from: u64,
    pub to: u64,
    pub query: String,
    pub line: usize,
    pub offset: usize,
    pub reverse: bool,
    pub topic: Option<String>,
    pub power_sql: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetLogsV2Response {
    pub progress: Option<String>,
    pub count: usize,
    pub logs: Vec<Value>,
    pub meta: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlsError {
    InvalidConfig(String),
    Signing(String),
    Network(String),
    Service {
        status: u16,
        code: Option<String>,
        message: String,
        request_id: Option<String>,
    },
    InvalidResponse(String),
}

impl std::fmt::Display for SlsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(formatter, "invalid SLS config: {message}"),
            Self::Signing(message) => write!(formatter, "failed to sign SLS request: {message}"),
            Self::Network(message) => write!(formatter, "SLS request failed: {message}"),
            Self::Service {
                status,
                code,
                message,
                request_id,
            } => {
                write!(formatter, "SLS returned HTTP {status}")?;
                if let Some(code) = code {
                    write!(formatter, " ({code})")?;
                }
                write!(formatter, ": {message}")?;
                if let Some(request_id) = request_id {
                    write!(formatter, " [requestId={request_id}]")?;
                }
                Ok(())
            }
            Self::InvalidResponse(message) => write!(formatter, "invalid SLS response: {message}"),
        }
    }
}

impl std::error::Error for SlsError {}

impl SlsClient {
    pub fn new(endpoint: &str, credentials: SlsCredentials) -> Result<Self, SlsError> {
        let endpoint = normalize_endpoint(endpoint)?;
        let http = Client::builder()
            .timeout(Duration::from_secs(SLS_REQUEST_TIMEOUT_SECONDS))
            .no_gzip()
            .build()
            .map_err(|error| SlsError::InvalidConfig(error.to_string()))?;

        Ok(Self {
            endpoint,
            credentials,
            http,
        })
    }

    pub async fn get_logs_v2(
        &self,
        request: &GetLogsV2Request,
    ) -> Result<GetLogsV2Response, SlsError> {
        let path = format!("/logstores/{}/logs", urlencoding::encode(&request.logstore));
        let query_params = [("project".to_string(), request.project.clone())];
        let body = get_logs_v2_body(request)?;
        let date = sls_date_now()?;
        let headers = signed_headers(
            "POST",
            &path,
            &query_params,
            &body,
            &date,
            &self.credentials,
        )?;
        let endpoint = endpoint_for_project(&self.endpoint, &request.project)?;
        let url = format!(
            "{}{}?project={}",
            endpoint,
            path,
            urlencoding::encode(&request.project)
        );

        let response = self
            .http
            .post(url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|error| SlsError::Network(error.to_string()))?;
        parse_get_logs_v2_response(response).await
    }
}

async fn parse_get_logs_v2_response(
    response: reqwest::Response,
) -> Result<GetLogsV2Response, SlsError> {
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-log-requestid")
        .or_else(|| response.headers().get("x-acs-request-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let content_encoding = response
        .headers()
        .get(header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let sls_compress_type = response
        .headers()
        .get("x-log-compresstype")
        .and_then(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| SlsError::Network(error.to_string()))?;
    let decoded = decode_response_body(&bytes, &content_encoding, &sls_compress_type)?;

    if !status.is_success() {
        let (code, message) = parse_service_error_body(&decoded);
        return Err(SlsError::Service {
            status: status.as_u16(),
            code,
            message,
            request_id,
        });
    }

    let value = serde_json::from_slice::<Value>(&decoded)
        .map_err(|error| SlsError::InvalidResponse(error.to_string()))?;
    parse_get_logs_v2_body(value)
}

fn parse_get_logs_v2_body(value: Value) -> Result<GetLogsV2Response, SlsError> {
    let meta = value.get("meta").cloned().unwrap_or_else(|| json!({}));
    let logs = value
        .get("data")
        .or_else(|| value.get("logs"))
        .and_then(Value::as_array)
        .ok_or_else(|| SlsError::InvalidResponse("missing data array".to_string()))?
        .clone();
    let progress = meta
        .get("progress")
        .or_else(|| value.get("progress"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let count = meta
        .get("count")
        .or_else(|| value.get("count"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(logs.len());

    Ok(GetLogsV2Response {
        progress,
        count,
        logs,
        meta,
    })
}

fn parse_service_error_body(bytes: &[u8]) -> (Option<String>, String) {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return (
            None,
            String::from_utf8_lossy(bytes).chars().take(512).collect(),
        );
    };
    let code = value
        .get("errorCode")
        .or_else(|| value.get("code"))
        .or_else(|| value.pointer("/Error/Code"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let message = value
        .get("errorMessage")
        .or_else(|| value.get("message"))
        .or_else(|| value.pointer("/Error/Message"))
        .and_then(Value::as_str)
        .unwrap_or("SLS request failed")
        .chars()
        .take(512)
        .collect();

    (code, message)
}

fn decode_response_body(
    bytes: &[u8],
    content_encoding: &str,
    sls_compress_type: &str,
) -> Result<Vec<u8>, SlsError> {
    let gzip_encoded = content_encoding
        .split(',')
        .map(str::trim)
        .any(|encoding| encoding.eq_ignore_ascii_case("gzip"))
        || sls_compress_type.eq_ignore_ascii_case("gzip")
        || bytes.starts_with(&[0x1f, 0x8b]);
    if !gzip_encoded {
        return Ok(bytes.to_vec());
    }

    let mut decoded = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        GzDecoder::new(bytes)
            .read_to_end(&mut decoded)
            .map_err(|error| {
                SlsError::InvalidResponse(format!(
                    "failed to decode gzip: {error}; contentEncoding={content_encoding:?}; xLogCompressType={sls_compress_type:?}; firstBytes={}",
                    first_bytes_hex(bytes, 8)
                ))
            })?;
    } else {
        ZlibDecoder::new(bytes)
            .read_to_end(&mut decoded)
            .map_err(|error| {
                SlsError::InvalidResponse(format!(
                    "failed to decode zlib: {error}; contentEncoding={content_encoding:?}; xLogCompressType={sls_compress_type:?}; firstBytes={}",
                    first_bytes_hex(bytes, 8)
                ))
            })?;
    }
    Ok(decoded)
}

fn first_bytes_hex(bytes: &[u8], max: usize) -> String {
    bytes
        .iter()
        .take(max)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn get_logs_v2_body(request: &GetLogsV2Request) -> Result<Vec<u8>, SlsError> {
    let mut body = serde_json::Map::new();
    body.insert("from".to_string(), json!(request.from));
    body.insert("to".to_string(), json!(request.to));
    body.insert("query".to_string(), json!(request.query));
    body.insert("line".to_string(), json!(request.line));
    body.insert("offset".to_string(), json!(request.offset));
    body.insert("reverse".to_string(), json!(request.reverse));
    body.insert("powerSql".to_string(), json!(request.power_sql));
    if let Some(topic) = &request.topic {
        body.insert("topic".to_string(), json!(topic));
    }

    serde_json::to_vec(&Value::Object(body))
        .map_err(|error| SlsError::InvalidResponse(error.to_string()))
}

fn signed_headers(
    method: &str,
    path: &str,
    query_params: &[(String, String)],
    body: &[u8],
    date: &str,
    credentials: &SlsCredentials,
) -> Result<HeaderMap, SlsError> {
    let content_md5 = md5_upper_hex(body);
    let body_len = body.len().to_string();
    let mut signing_headers = BTreeMap::new();
    signing_headers.insert("x-log-apiversion".to_string(), SLS_API_VERSION.to_string());
    signing_headers.insert("x-log-bodyrawsize".to_string(), body_len.clone());
    signing_headers.insert(
        "x-log-signaturemethod".to_string(),
        SLS_SIGNATURE_METHOD.to_string(),
    );
    if let Some(token) = credentials.security_token.as_deref()
        && !token.trim().is_empty()
    {
        signing_headers.insert("x-acs-security-token".to_string(), token.to_string());
    }

    let signature = sign_request(
        method,
        &content_md5,
        SLS_CONTENT_TYPE,
        date,
        &signing_headers,
        path,
        query_params,
        &credentials.access_key_secret,
    )?;
    let authorization = format!("LOG {}:{signature}", credentials.access_key_id);

    let mut headers = HeaderMap::new();
    insert_header(&mut headers, header::DATE, date)?;
    insert_header(&mut headers, header::CONTENT_TYPE, SLS_CONTENT_TYPE)?;
    insert_header(&mut headers, header::ACCEPT, SLS_CONTENT_TYPE)?;
    insert_header(
        &mut headers,
        HeaderName::from_static("content-md5"),
        &content_md5,
    )?;
    insert_header(&mut headers, header::ACCEPT_ENCODING, SLS_ACCEPT_ENCODING)?;
    insert_header(
        &mut headers,
        HeaderName::from_static("x-log-apiversion"),
        SLS_API_VERSION,
    )?;
    insert_header(
        &mut headers,
        HeaderName::from_static("x-log-bodyrawsize"),
        &body_len,
    )?;
    insert_header(
        &mut headers,
        HeaderName::from_static("x-log-signaturemethod"),
        SLS_SIGNATURE_METHOD,
    )?;
    if let Some(token) = credentials.security_token.as_deref()
        && !token.trim().is_empty()
    {
        insert_header(
            &mut headers,
            HeaderName::from_static("x-acs-security-token"),
            token,
        )?;
    }
    insert_header(&mut headers, header::AUTHORIZATION, &authorization)?;

    Ok(headers)
}

#[allow(clippy::too_many_arguments)]
fn sign_request(
    method: &str,
    content_md5: &str,
    content_type: &str,
    date: &str,
    signing_headers: &BTreeMap<String, String>,
    path: &str,
    query_params: &[(String, String)],
    access_key_secret: &str,
) -> Result<String, SlsError> {
    let mut message = format!("{method}\n{content_md5}\n{content_type}\n{date}\n");
    for (name, value) in signing_headers {
        message.push_str(name);
        message.push(':');
        message.push_str(value);
        message.push('\n');
    }
    message.push_str(path);
    if !query_params.is_empty() {
        message.push('?');
        let mut sorted = query_params.to_vec();
        sorted.sort_by(|left, right| left.0.cmp(&right.0));
        for (index, (name, value)) in sorted.iter().enumerate() {
            if index > 0 {
                message.push('&');
            }
            message.push_str(name);
            message.push('=');
            message.push_str(value);
        }
    }

    let mut mac = HmacSha1::new_from_slice(access_key_secret.as_bytes())
        .map_err(|error| SlsError::Signing(error.to_string()))?;
    mac.update(message.as_bytes());
    Ok(BASE64_STANDARD.encode(mac.finalize().into_bytes()))
}

fn insert_header(headers: &mut HeaderMap, name: HeaderName, value: &str) -> Result<(), SlsError> {
    let value =
        HeaderValue::from_str(value).map_err(|error| SlsError::Signing(error.to_string()))?;
    headers.insert(name, value);
    Ok(())
}

fn md5_upper_hex(bytes: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

fn sls_date_now() -> Result<String, SlsError> {
    let format = format_description::parse(
        "[weekday repr:short], [day padding:zero] [month repr:short] [year] [hour padding:zero]:[minute padding:zero]:[second padding:zero] GMT",
    )
    .map_err(|error| SlsError::Signing(error.to_string()))?;
    OffsetDateTime::now_utc()
        .format(&format)
        .map_err(|error| SlsError::Signing(error.to_string()))
}

fn normalize_endpoint(endpoint: &str) -> Result<String, SlsError> {
    let endpoint = endpoint.trim().trim_end_matches('/');
    if endpoint.is_empty() {
        return Err(SlsError::InvalidConfig(
            "endpoint must not be empty".to_string(),
        ));
    }
    if endpoint.chars().any(char::is_whitespace) {
        return Err(SlsError::InvalidConfig(
            "endpoint must not contain whitespace".to_string(),
        ));
    }
    let endpoint = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_string()
    } else {
        format!("https://{endpoint}")
    };
    let parsed =
        Url::parse(&endpoint).map_err(|error| SlsError::InvalidConfig(error.to_string()))?;
    if parsed.host_str().is_none() {
        return Err(SlsError::InvalidConfig(
            "endpoint must include a host".to_string(),
        ));
    }
    if parsed.path() != "/" {
        return Err(SlsError::InvalidConfig(
            "endpoint must not include a path".to_string(),
        ));
    }

    Ok(endpoint)
}

fn endpoint_for_project(endpoint: &str, project: &str) -> Result<String, SlsError> {
    let mut url =
        Url::parse(endpoint).map_err(|error| SlsError::InvalidConfig(error.to_string()))?;
    let Some(host) = url.host_str().map(str::to_string) else {
        return Err(SlsError::InvalidConfig(
            "endpoint must include a host".to_string(),
        ));
    };
    if host == "localhost"
        || host.parse::<IpAddr>().is_ok()
        || host.starts_with(&format!("{project}."))
    {
        return Ok(endpoint.trim_end_matches('/').to_string());
    }

    url.set_host(Some(&format!("{project}.{host}")))
        .map_err(|_| {
            SlsError::InvalidConfig("failed to build project endpoint host".to_string())
        })?;
    Ok(url.as_str().trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{
        Compression,
        write::{GzEncoder, ZlibEncoder},
    };
    use std::io::Write;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::oneshot,
    };

    fn test_request() -> GetLogsV2Request {
        GetLogsV2Request {
            project: "ali-test-project".to_string(),
            logstore: "test-logstore".to_string(),
            from: 1_627_268_185,
            to: 1_627_268_245,
            query: "status: 401 | select count(*) as pv".to_string(),
            line: 0,
            offset: 0,
            reverse: true,
            topic: Some("topic-a".to_string()),
            power_sql: true,
        }
    }

    fn test_credentials() -> SlsCredentials {
        SlsCredentials {
            access_key_id: "test-ak".to_string(),
            access_key_secret: "test-secret".to_string(),
            security_token: Some("test-token".to_string()),
        }
    }

    #[test]
    fn signs_sls_requests_with_expected_canonical_parts() {
        let body = get_logs_v2_body(&test_request()).expect("body should encode");
        let headers = signed_headers(
            "POST",
            "/logstores/test-logstore/logs",
            &[("project".to_string(), "ali-test-project".to_string())],
            &body,
            "Tue, 23 Aug 2022 12:12:03 GMT",
            &test_credentials(),
        )
        .expect("headers should sign");

        assert_eq!(headers["x-log-apiversion"], SLS_API_VERSION);
        assert_eq!(headers["x-log-signaturemethod"], SLS_SIGNATURE_METHOD);
        assert_eq!(headers["x-acs-security-token"], "test-token");
        assert_eq!(headers["accept-encoding"], SLS_ACCEPT_ENCODING);
        assert_eq!(headers["content-md5"], md5_upper_hex(&body));
        assert!(
            headers["authorization"]
                .to_str()
                .unwrap()
                .starts_with("LOG test-ak:")
        );
    }

    #[test]
    fn builds_project_host_from_regional_endpoint() {
        assert_eq!(
            endpoint_for_project("https://cn-shanghai.log.aliyuncs.com", "sample-project").unwrap(),
            "https://sample-project.cn-shanghai.log.aliyuncs.com"
        );
        assert_eq!(
            endpoint_for_project(
                "https://sample-project.cn-shanghai.log.aliyuncs.com",
                "sample-project"
            )
            .unwrap(),
            "https://sample-project.cn-shanghai.log.aliyuncs.com"
        );
        assert_eq!(
            endpoint_for_project("http://127.0.0.1:8080", "sample-project").unwrap(),
            "http://127.0.0.1:8080"
        );
    }

    #[tokio::test]
    async fn get_logs_v2_sends_signed_request_and_parses_success() {
        let response =
            br#"{"meta":{"progress":"Complete","count":1},"data":[{"message":"ok"}]}"#.to_vec();
        let (endpoint, request_rx) = spawn_server(response, &[]).await;
        let client = SlsClient::new(&endpoint, test_credentials()).expect("client should build");

        let result = client
            .get_logs_v2(&test_request())
            .await
            .expect("query should succeed");
        let request = request_rx.await.expect("server should capture request");

        assert!(
            request.starts_with("POST /logstores/test-logstore/logs?project=ali-test-project ")
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("accept-encoding: gzip")
        );
        assert!(request.to_ascii_lowercase().contains("content-md5: "));
        let request_lower = request.to_ascii_lowercase();
        assert!(request_lower.contains("authorization: log test-ak:"));
        assert!(request_lower.contains("x-acs-security-token: test-token"));
        let body = request_body(&request);
        let body = serde_json::from_str::<Value>(body).expect("request body should be json");
        assert_eq!(body["from"], 1_627_268_185);
        assert_eq!(body["to"], 1_627_268_245);
        assert_eq!(body["query"], "status: 401 | select count(*) as pv");
        assert_eq!(body["powerSql"], true);

        assert_eq!(result.progress.as_deref(), Some("Complete"));
        assert_eq!(result.count, 1);
        assert_eq!(result.logs[0]["message"], "ok");
    }

    #[tokio::test]
    async fn get_logs_v2_parses_gzip_response() {
        let body = br#"{"meta":{"progress":"Complete","count":1},"data":[{"message":"gz"}]}"#;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(body).expect("gzip write should work");
        let compressed = encoder.finish().expect("gzip finish should work");
        let (endpoint, _request_rx) =
            spawn_server(compressed, &[("Content-Encoding", "gzip")]).await;
        let client = SlsClient::new(&endpoint, test_credentials()).expect("client should build");

        let result = client
            .get_logs_v2(&test_request())
            .await
            .expect("query should succeed");

        assert_eq!(result.logs[0]["message"], "gz");
    }

    #[tokio::test]
    async fn get_logs_v2_parses_zlib_response_marked_as_gzip() {
        let body = br#"{"meta":{"progress":"Complete","count":1},"data":[{"message":"zlib"}]}"#;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(body).expect("zlib write should work");
        let compressed = encoder.finish().expect("zlib finish should work");
        let (endpoint, _request_rx) =
            spawn_server(compressed, &[("x-log-compresstype", "gzip")]).await;
        let client = SlsClient::new(&endpoint, test_credentials()).expect("client should build");

        let result = client
            .get_logs_v2(&test_request())
            .await
            .expect("query should succeed");

        assert_eq!(result.logs[0]["message"], "zlib");
    }

    #[tokio::test]
    async fn get_logs_v2_maps_sls_error_response() {
        let response =
            br#"{"errorCode":"ProjectNotExist","errorMessage":"missing project"}"#.to_vec();
        let (endpoint, _request_rx) =
            spawn_server_with_status(response, &[], "403 Forbidden").await;
        let client = SlsClient::new(&endpoint, test_credentials()).expect("client should build");

        let error = client
            .get_logs_v2(&test_request())
            .await
            .expect_err("query should fail");

        assert_eq!(
            error,
            SlsError::Service {
                status: 403,
                code: Some("ProjectNotExist".to_string()),
                message: "missing project".to_string(),
                request_id: None,
            }
        );
    }

    #[tokio::test]
    async fn get_logs_v2_rejects_non_json_success_response() {
        let (endpoint, _request_rx) = spawn_server(b"not-json".to_vec(), &[]).await;
        let client = SlsClient::new(&endpoint, test_credentials()).expect("client should build");

        let error = client
            .get_logs_v2(&test_request())
            .await
            .expect_err("query should fail");

        assert!(matches!(error, SlsError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn get_logs_v2_maps_network_errors() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let client = SlsClient::new(&endpoint, test_credentials()).expect("client should build");

        let error = client
            .get_logs_v2(&test_request())
            .await
            .expect_err("query should fail");

        assert!(matches!(error, SlsError::Network(_)));
    }

    async fn spawn_server(
        response: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> (String, oneshot::Receiver<String>) {
        spawn_server_with_status(response, headers, "200 OK").await
    }

    async fn spawn_server_with_status(
        response: Vec<u8>,
        headers: &[(&str, &str)],
        status: &'static str,
    ) -> (String, oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (request_tx, request_rx) = oneshot::channel();
        let headers = headers
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect::<Vec<_>>();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("server should accept");
            let request = read_http_request(&mut socket)
                .await
                .expect("server should read request");
            let _ = request_tx.send(request);
            let mut response_head = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                response.len()
            );
            for (name, value) in headers {
                response_head.push_str(&format!("{name}: {value}\r\n"));
            }
            response_head.push_str("\r\n");
            socket
                .write_all(response_head.as_bytes())
                .await
                .expect("server should write response head");
            socket
                .write_all(&response)
                .await
                .expect("server should write response body");
        });

        (endpoint, request_rx)
    }

    async fn read_http_request(socket: &mut tokio::net::TcpStream) -> std::io::Result<String> {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let read = socket.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if http_request_complete(&buffer) {
                break;
            }
        }

        Ok(String::from_utf8_lossy(&buffer).to_string())
    }

    fn http_request_complete(buffer: &[u8]) -> bool {
        let Some(header_end) = find_header_end(buffer) else {
            return false;
        };
        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or_default();
        buffer.len() >= header_end + 4 + content_length
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn request_body(request: &str) -> &str {
        request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .unwrap_or("")
    }
}
