#!/usr/bin/env bash
if [ -z "${BASH_VERSION:-}" ]; then
    echo "This script requires bash. Run: bash scripts/smoke-demo-stack.sh" >&2
    exit 2
fi

set -euo pipefail

MCP_HOST="${MCP_HOST:-127.0.0.1}"
MCP_PORT="${MCP_PORT:-8080}"
MCP_URL="${MCP_URL:-http://${MCP_HOST}:${MCP_PORT}/mcp}"
TOKEN_FILE="${TOKEN_FILE:-.local/run/action-gateway-token}"

resolve_local_token() {
    if [[ -n "${ACTION_GATEWAY_MCP_TOKEN:-}" ]]; then
        echo "${ACTION_GATEWAY_MCP_TOKEN}"
        return
    fi

    if [[ -n "${RPC_TOKEN:-}" ]]; then
        echo "${RPC_TOKEN}"
        return
    fi

    if [[ -f "${TOKEN_FILE}" ]]; then
        cat "${TOKEN_FILE}"
        return
    fi

    echo "Missing ACTION_GATEWAY_MCP_TOKEN/RPC_TOKEN and ${TOKEN_FILE} was not found. Start the demo stack first or export a token." >&2
    exit 1
}

ACTION_GATEWAY_MCP_TOKEN="$(resolve_local_token)"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

require_cmd() {
    local cmd="$1"

    if ! command -v "${cmd}" >/dev/null 2>&1; then
        echo "Missing required command: ${cmd}" >&2
        exit 1
    fi
}

mcp_request() {
    local name="$1"
    local payload="$2"
    local output="${TMP_DIR}/${name}.json"

    curl -fsS --max-time 8 "${MCP_URL}" \
        -H 'Content-Type: application/json' \
        -H "Authorization: Bearer ${ACTION_GATEWAY_MCP_TOKEN}" \
        -H "X-Request-Id: smoke-${name}" \
        -H "X-Actor-Id: smoke" \
        -H "X-Actor-Role: local" \
        -d "${payload}" \
        > "${output}" || return 1

    printf '%s\n' "${output}"
}

assert_json() {
    local label="$1"
    local file="$2"
    local expression="$3"

    node -e '
const fs = require("node:fs");
const label = process.argv[1];
const file = process.argv[2];
const expression = process.argv[3];
const data = JSON.parse(fs.readFileSync(file, "utf8"));
let ok = false;
try {
  ok = Boolean(Function("data", `return (${expression});`)(data));
} catch (error) {
  console.error(`${label}: assertion threw: ${error.message}`);
  process.exit(1);
}
if (!ok) {
  console.error(`${label}: assertion failed`);
  console.error(JSON.stringify(data, null, 2));
  process.exit(1);
}
' "${label}" "${file}" "${expression}"
}

require_cmd curl
require_cmd node

echo "Smoke checking MCP initialize..."
initialize_file="$(mcp_request initialize '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"smoke-demo-stack","version":"0.1.0"}}}')"
assert_json "initialize" "${initialize_file}" 'data.result?.protocolVersion === "2025-11-25"'

echo "Smoke checking tools/list..."
tools_file="$(mcp_request tools '{"jsonrpc":"2.0","id":2,"method":"tools/list"}')"
assert_json "tools/list" "${tools_file}" '
Array.isArray(data.result?.tools) &&
[
  "data.query_table",
  "redis.query_key",
  "kubernetes.list_resources",
  "kubernetes.get_resource",
  "kubernetes.rollout_status",
  "kubernetes.query_pod_logs",
  "logs.query_app_logs",
  "audit.query_approval_events",
].every((name) => data.result.tools.some((tool) => tool.name === name)) &&
!data.result.tools.some((tool) => tool.name === "kubernetes.kubectl_read")
'

echo "Smoke checking Redis key query..."
redis_file="$(mcp_request redis '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"redis.query_key","arguments":{"key":"demo:user:1","limit":10}}}')"
assert_json "redis.query_key" "${redis_file}" 'data.result?.isError === false && data.result.structuredContent?.status === "succeeded" && data.result.structuredContent.exists === true'

echo "Smoke checking application log query..."
logs_file="$(mcp_request logs '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"logs.query_app_logs","arguments":{"app_name":"billing-api","environment":"prod","keyword":"12.00","limit":5}}}')"
assert_json "logs.query_app_logs" "${logs_file}" 'data.result?.isError === false && data.result.structuredContent?.status === "succeeded" && data.result.structuredContent.returnedCount >= 1'

echo "Smoke checking application log trace and truncation filters..."
logs_trace_file="$(mcp_request logs_trace '{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"logs.query_app_logs","arguments":{"app_name":"billing-api","trace_id":"trc_paid_summary_001","limit":10}}}')"
assert_json "logs.query_app_logs trace_id" "${logs_trace_file}" 'data.result?.isError === false && data.result.structuredContent?.status === "succeeded" && data.result.structuredContent.returnedCount >= 2'

logs_limit_file="$(mcp_request logs_limit '{"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":"logs.query_app_logs","arguments":{"app_name":"billing-api","limit":1}}}')"
assert_json "logs.query_app_logs limit" "${logs_limit_file}" 'data.result?.isError === false && data.result.structuredContent?.status === "succeeded" && data.result.structuredContent.returnedCount === 1 && data.result.structuredContent.truncated === true'

logs_missing_file="$(mcp_request logs_missing '{"jsonrpc":"2.0","id":43,"method":"tools/call","params":{"name":"logs.query_app_logs","arguments":{"app_name":"missing-api","limit":5}}}')"
assert_json "logs.query_app_logs missing index" "${logs_missing_file}" 'data.result?.isError === true && data.result.structuredContent?.status === "not_allowed"'

echo "Smoke checking audit query..."
audit_file="$(mcp_request audit '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"audit.query_approval_events","arguments":{"actor_id":"smoke","limit":20}}}')"
assert_json "audit.query_approval_events" "${audit_file}" 'data.result?.isError === false && data.result.structuredContent?.status === "succeeded" && data.result.structuredContent.eventCount >= 1'

echo "Smoke check passed."
