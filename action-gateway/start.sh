#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

detect_access_host() {
    local detected=""

    if [[ -n "${ACCESS_HOST:-}" ]]; then
        echo "${ACCESS_HOST}"
        return
    fi

    if command -v ip >/dev/null 2>&1; then
        detected="$(ip route get 1.1.1.1 2>/dev/null | awk '{for (i = 1; i <= NF; i++) if ($i == "src") {print $(i + 1); exit}}')"
        if [[ -n "${detected}" ]]; then
            echo "${detected}"
            return
        fi
    fi

    if command -v hostname >/dev/null 2>&1; then
        detected="$(hostname -I 2>/dev/null | awk '{print $1}')"
        if [[ -n "${detected}" ]]; then
            echo "${detected}"
            return
        fi
    fi

    echo "${BIND_HOST}"
}

port_in_use() {
    local port="$1"
    local listeners=""

    if command -v ss >/dev/null 2>&1; then
        listeners="$(ss -ltn 2>/dev/null || true)"
        if [[ -n "${listeners}" ]]; then
            printf '%s\n' "${listeners}" | awk '{print $4}' | grep -Eq "[:.]${port}$"
            return
        fi
    fi

    (echo >/dev/tcp/127.0.0.1/"${port}") >/dev/null 2>&1
}

choose_free_port() {
    local preferred="$1"
    local start="$2"
    local end="$3"
    local port

    if ! port_in_use "${preferred}"; then
        echo "${preferred}"
        return
    fi

    for port in $(seq "${start}" "${end}"); do
        if ! port_in_use "${port}"; then
            echo "Port ${preferred} is in use; using ${port} instead." >&2
            echo "${port}"
            return
        fi
    done

    echo "No free port found in ${start}-${end}." >&2
    exit 1
}

require_cmd() {
    local cmd="$1"
    if ! command -v "${cmd}" >/dev/null 2>&1; then
        echo "Missing required command: ${cmd}" >&2
        exit 1
    fi
}

ensure_store_file() {
    if [[ -f "${GATEWAY_STORE_FILE}" ]]; then
        return
    fi

    mkdir -p "$(dirname "${GATEWAY_STORE_FILE}")"
    cp "${ROOT_DIR}/gateway-store.example.json" "${GATEWAY_STORE_FILE}"
    echo "Created file store: ${GATEWAY_STORE_FILE}"
}

BIND_HOST="${BIND_HOST:-0.0.0.0}"
BACKEND_HOST="${BACKEND_HOST:-${BIND_HOST}}"
BACKEND_PORT_WAS_SET="${BACKEND_PORT+x}"
BACKEND_PORT="${BACKEND_PORT:-8080}"

if [[ -z "${BACKEND_PORT_WAS_SET}" ]]; then
    BACKEND_PORT="$(choose_free_port "${BACKEND_PORT}" 8081 8099)"
fi

ACCESS_HOST="$(detect_access_host)"

export RPC_BIND_ADDR="${RPC_BIND_ADDR:-${BACKEND_HOST}:${BACKEND_PORT}}"
export GATEWAY_STORE_FILE="${GATEWAY_STORE_FILE:-${ROOT_DIR}/.local/run/gateway-store.json}"
export REDIS_URL="${REDIS_URL:-redis://127.0.0.1:6379/}"
export RPC_TOKEN="${RPC_TOKEN:-${ACTION_GATEWAY_MCP_TOKEN:-Xbcd20198\$}}"
export GATEWAY_ALLOW_LEGACY_RPC_TOKEN="${GATEWAY_ALLOW_LEGACY_RPC_TOKEN:-true}"

require_cmd cargo
ensure_store_file

echo "Starting Gateway: http://${ACCESS_HOST}:${BACKEND_PORT}/mcp"
echo "Admin JSON API:  http://${ACCESS_HOST}:${BACKEND_PORT}/admin"
echo "File store:      ${GATEWAY_STORE_FILE}"
echo
echo "Press Ctrl+C to stop."

cd "${ROOT_DIR}"
cargo run
