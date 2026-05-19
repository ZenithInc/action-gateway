#!/usr/bin/env bash
if [ -z "${BASH_VERSION:-}" ]; then
    echo "This script requires bash. Run: bash scripts/start-demo-stack.sh [start|stop|restart|status]" >&2
    exit 2
fi

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_DIR="${STATE_DIR:-${ROOT_DIR}/.local/run}"
LOG_DIR="${LOG_DIR:-${ROOT_DIR}/.local/logs}"

GATEWAY_PID_FILE="${STATE_DIR}/action-gateway.pid"
GATEWAY_LOG="${LOG_DIR}/action-gateway.log"

ACTION="${1:-start}"

usage() {
    cat <<'USAGE'
Usage: scripts/start-demo-stack.sh [start|stop|restart|status]

Environment overrides:
  REDIS_PORT                 Host port for the Docker Redis service.
  MCP_HOST                   Gateway host, default 127.0.0.1.
  MCP_PORT                   Gateway port, default 8080.
  GATEWAY_STORE_FILE         JSON file used for Gateway state.
  ACTION_GATEWAY_MCP_TOKEN   Token used by Codex and the gateway.
  RPC_TOKEN                  Gateway token. Defaults to ACTION_GATEWAY_MCP_TOKEN.
  STOP_INFRA=1               Also stop Docker Redis on stop.
USAGE
}

require_cmd() {
    local cmd="$1"

    if ! command -v "${cmd}" >/dev/null 2>&1; then
        echo "Missing required command: ${cmd}" >&2
        exit 1
    fi
}

compose() {
    docker compose -f "${ROOT_DIR}/docker-compose.yml" "$@"
}

compose_port() {
    local service="$1"
    local container_port="$2"
    local output

    output="$(compose port "${service}" "${container_port}" 2>/dev/null || true)"
    printf '%s\n' "${output}" \
        | sed -E 's/.*:([0-9]+)$/\1/' \
        | tail -n 1
}

port_in_use() {
    local port="$1"
    local listeners

    if command -v ss >/dev/null 2>&1; then
        listeners="$(ss -ltn 2>/dev/null || true)"
        if [[ -n "${listeners}" ]]; then
            printf '%s\n' "${listeners}" | awk '{print $4}' | grep -Eq "[:.]${port}$"
            return
        fi
    fi

    (echo >/dev/tcp/127.0.0.1/"${port}") >/dev/null 2>&1
}

find_free_port() {
    local preferred="$1"
    local start="$2"
    local end="$3"
    local port

    if ! port_in_use "${preferred}"; then
        echo "${preferred}"
        return 0
    fi

    for port in $(seq "${start}" "${end}"); do
        if ! port_in_use "${port}"; then
            echo "${port}"
            return 0
        fi
    done

    echo "No free port found in ${start}-${end}" >&2
    return 1
}

choose_redis_port() {
    local existing_port

    if [[ -n "${REDIS_PORT:-}" ]]; then
        echo "${REDIS_PORT}"
        return
    fi

    existing_port="$(compose_port redis 6379)"
    if [[ -n "${existing_port}" ]]; then
        echo "${existing_port}"
        return
    fi

    find_free_port 6379 6381 6390
}

wait_for_compose_health() {
    local service="$1"
    local container_id
    local status

    echo "Waiting for ${service} to become healthy..."
    for _ in $(seq 1 60); do
        container_id="$(compose ps -q "${service}")"
        if [[ -n "${container_id}" ]]; then
            status="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${container_id}" 2>/dev/null || true)"
            if [[ "${status}" == "healthy" || "${status}" == "running" ]]; then
                return 0
            fi
        fi
        sleep 1
    done

    echo "${service} did not become healthy in time." >&2
    compose ps "${service}" >&2 || true
    return 1
}

mcp_request() {
    local payload="$1"

    curl -fsS --max-time 3 "http://${MCP_HOST}:${MCP_PORT}/mcp" \
        -H 'Content-Type: application/json' \
        -H "Authorization: Bearer ${ACTION_GATEWAY_MCP_TOKEN}" \
        -d "${payload}"
}

mcp_initialize_ok() {
    mcp_request '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"local-start-demo-stack","version":"0.1.0"}}}' >/dev/null 2>&1
}

mcp_tools_current_ok() {
    local response

    response="$(mcp_request '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' 2>/dev/null)" || return 1

    node -e '
const response = JSON.parse(process.argv[1]);
const tools = response.result?.tools;
if (!Array.isArray(tools)) process.exit(1);
const names = tools.map((tool) => tool.name);
const required = [
  "data.query_table",
  "redis.query_key",
  "kubernetes.list_resources",
  "kubernetes.get_resource",
  "kubernetes.rollout_status",
  "kubernetes.query_pod_logs",
  "logs.query_app_logs",
  "audit.query_approval_events",
];
if (!required.every((name) => names.includes(name))) process.exit(1);
const rawEnabled = /^(true|1|yes)$/i.test(process.env.KUBERNETES_ENABLE_RAW_KUBECTL ?? "");
if (names.includes("kubernetes.kubectl_read") !== rawEnabled) process.exit(1);
' "${response}" >/dev/null 2>&1
}

mcp_ready_ok() {
    mcp_initialize_ok && mcp_tools_current_ok
}

wait_for_http() {
    local name="$1"
    local pid_file="$2"
    local check_fn="$3"
    local log_file="$4"

    echo "Waiting for ${name}..."
    for _ in $(seq 1 180); do
        if "${check_fn}"; then
            return 0
        fi

        if [[ -f "${pid_file}" ]]; then
            local pid
            pid="$(cat "${pid_file}")"
            if [[ -n "${pid}" ]] && ! kill -0 "${pid}" >/dev/null 2>&1; then
                echo "${name} exited before it became ready. Log: ${log_file}" >&2
                tail -n 80 "${log_file}" >&2 || true
                return 1
            fi
        fi

        sleep 1
    done

    echo "${name} did not become ready in time. Log: ${log_file}" >&2
    tail -n 80 "${log_file}" >&2 || true
    return 1
}

process_from_pid_file_running() {
    local pid_file="$1"
    local pid

    [[ -f "${pid_file}" ]] || return 1
    pid="$(cat "${pid_file}")"
    [[ -n "${pid}" ]] || return 1
    kill -0 "${pid}" >/dev/null 2>&1
}

stop_process() {
    local name="$1"
    local pid_file="$2"
    local pid

    if ! [[ -f "${pid_file}" ]]; then
        echo "${name} is not managed by this script."
        return
    fi

    pid="$(cat "${pid_file}")"
    if [[ -z "${pid}" ]] || ! kill -0 "${pid}" >/dev/null 2>&1; then
        rm -f "${pid_file}"
        echo "${name} is not running."
        return
    fi

    echo "Stopping ${name}..."
    kill -- "-${pid}" >/dev/null 2>&1 || kill "${pid}" >/dev/null 2>&1 || true

    for _ in $(seq 1 20); do
        if ! kill -0 "${pid}" >/dev/null 2>&1; then
            rm -f "${pid_file}"
            return
        fi
        sleep 1
    done

    kill -KILL -- "-${pid}" >/dev/null 2>&1 || kill -KILL "${pid}" >/dev/null 2>&1 || true
    rm -f "${pid_file}"
}

ensure_store_file() {
    if [[ -f "${GATEWAY_STORE_FILE}" ]]; then
        return
    fi

    mkdir -p "$(dirname "${GATEWAY_STORE_FILE}")"
    cp "${ROOT_DIR}/gateway-store.example.json" "${GATEWAY_STORE_FILE}"
    echo "Created file store: ${GATEWAY_STORE_FILE}"
}

stop_stack() {
    stop_process "action-gateway" "${GATEWAY_PID_FILE}"

    if [[ "${STOP_INFRA:-0}" == "1" ]]; then
        compose stop redis
    fi
}

start_infra() {
    export REDIS_PORT

    echo "Starting Docker Redis on ${REDIS_PORT}..."
    compose up -d redis
    wait_for_compose_health redis

    echo "Seeding demo Redis data..."
    "${ROOT_DIR}/scripts/seed-fake-redis-data.sh"
}

start_gateway() {
    if mcp_initialize_ok; then
        if mcp_tools_current_ok; then
            echo "action-gateway is already responding at http://${MCP_HOST}:${MCP_PORT}/mcp."
            return
        fi

        echo "action-gateway is responding at http://${MCP_HOST}:${MCP_PORT}/mcp, but tools/list does not match the current source." >&2
        echo "Stop the stale process on that port or set MCP_PORT to a free port." >&2
        return 1
    fi

    if port_in_use "${MCP_PORT}"; then
        echo "Port ${MCP_PORT} is already in use, but MCP initialize failed." >&2
        echo "Stop the process on that port or set MCP_PORT to a free port." >&2
        return 1
    fi

    ensure_store_file
    rm -f "${GATEWAY_PID_FILE}"
    echo "Starting action-gateway MCP on http://${MCP_HOST}:${MCP_PORT}/mcp..."
    (
        cd "${ROOT_DIR}"
        setsid env \
            GATEWAY_STORE_FILE="${GATEWAY_STORE_FILE}" \
            REDIS_URL="${REDIS_URL}" \
            RPC_BIND_ADDR="${RPC_BIND_ADDR}" \
            RPC_TOKEN="${RPC_TOKEN}" \
            GATEWAY_ALLOW_LEGACY_RPC_TOKEN=true \
            KUBERNETES_ENABLE_RAW_KUBECTL="${KUBERNETES_ENABLE_RAW_KUBECTL:-false}" \
            cargo run >"${GATEWAY_LOG}" 2>&1 &
        echo "$!" >"${GATEWAY_PID_FILE}"
    )

    wait_for_http "action-gateway MCP" "${GATEWAY_PID_FILE}" mcp_ready_ok "${GATEWAY_LOG}"
}

print_status() {
    echo "Docker services:"
    compose ps redis || true
    echo

    if mcp_ready_ok; then
        echo "action-gateway MCP: ready at http://${MCP_HOST}:${MCP_PORT}/mcp"
    elif mcp_initialize_ok; then
        echo "action-gateway MCP: responding, but tools/list does not match the current source"
    elif process_from_pid_file_running "${GATEWAY_PID_FILE}"; then
        echo "action-gateway MCP: process running, not ready yet. Log: ${GATEWAY_LOG}"
    else
        echo "action-gateway MCP: not running"
    fi

    echo "Gateway store: ${GATEWAY_STORE_FILE}"
}

start_stack() {
    start_infra
    start_gateway

    echo
    echo "Ready."
    echo "  MCP endpoint: http://${MCP_HOST}:${MCP_PORT}/mcp"
    echo "  Admin JSON API: http://${MCP_HOST}:${MCP_PORT}/admin"
    echo "  Gateway store: ${GATEWAY_STORE_FILE}"
    echo "  Logs: ${LOG_DIR}"
    echo "  Smoke check: scripts/smoke-demo-stack.sh"
    echo
    echo "Before starting Codex, use:"
    printf '  export ACTION_GATEWAY_MCP_TOKEN=%q\n' "${ACTION_GATEWAY_MCP_TOKEN}"
}

case "${ACTION}" in
    start)
        ;;
    stop)
        mkdir -p "${STATE_DIR}" "${LOG_DIR}"
        stop_stack
        exit 0
        ;;
    restart)
        mkdir -p "${STATE_DIR}" "${LOG_DIR}"
        stop_stack
        ;;
    status)
        mkdir -p "${STATE_DIR}" "${LOG_DIR}"
        ;;
    -h|--help|help)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 1
        ;;
esac

require_cmd docker
require_cmd curl
require_cmd sed
require_cmd tail
require_cmd cargo
require_cmd node
require_cmd setsid

mkdir -p "${STATE_DIR}" "${LOG_DIR}"

if [[ -n "${RPC_BIND_ADDR:-}" ]]; then
    MCP_HOST="${RPC_BIND_ADDR%:*}"
    MCP_PORT="${RPC_BIND_ADDR##*:}"
else
    MCP_HOST="${MCP_HOST:-127.0.0.1}"
    MCP_PORT="${MCP_PORT:-8080}"
fi

REDIS_PORT="$(choose_redis_port)"
REDIS_URL="${REDIS_URL:-redis://127.0.0.1:${REDIS_PORT}/}"
RPC_BIND_ADDR="${MCP_HOST}:${MCP_PORT}"
ACTION_GATEWAY_MCP_TOKEN="${ACTION_GATEWAY_MCP_TOKEN:-${RPC_TOKEN:-Xbcd20198\$}}"
RPC_TOKEN="${RPC_TOKEN:-${ACTION_GATEWAY_MCP_TOKEN}}"
GATEWAY_STORE_FILE="${GATEWAY_STORE_FILE:-${STATE_DIR}/gateway-store.json}"

export ACTION_GATEWAY_MCP_TOKEN

case "${ACTION}" in
    status)
        print_status
        ;;
    start|restart)
        start_stack
        ;;
esac
