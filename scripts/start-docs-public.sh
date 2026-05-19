#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIND_HOST="${BIND_HOST:-0.0.0.0}"
DOCS_PORT_WAS_SET="${DOCS_PORT+x}${PORT+x}"
DOCS_PORT="${DOCS_PORT:-${PORT:-5174}}"

detect_public_host() {
    local detected=""

    if [[ -n "${PUBLIC_HOST:-}" ]]; then
        echo "${PUBLIC_HOST}"
        return
    fi

    if command -v curl >/dev/null 2>&1; then
        detected="$(curl --max-time 5 -fsS ip.sb 2>/dev/null | tr -d '[:space:]' || true)"
        if [[ -n "${detected}" ]]; then
            echo "${detected}"
            return
        fi
    fi

    echo "${BIND_HOST}"
}

format_url_host() {
    local host="$1"

    if [[ "${host}" == *:* && "${host}" != \[*\] ]]; then
        echo "[${host}]"
        return
    fi

    echo "${host}"
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

    echo "No free docs port found in ${start}-${end}." >&2
    exit 1
}

if [[ -z "${DOCS_PORT_WAS_SET}" ]]; then
    DOCS_PORT="$(choose_free_port "${DOCS_PORT}" 5175 5199)"
fi

PUBLIC_HOST="$(detect_public_host)"
URL_HOST="$(format_url_host "${PUBLIC_HOST}")"

if [[ ! -x "${ROOT_DIR}/node_modules/.bin/vitepress" ]]; then
    echo "Missing local VitePress dependency. Run npm install first." >&2
    exit 1
fi

echo "Starting docs: http://${URL_HOST}:${DOCS_PORT}/"
echo "Binding:       ${BIND_HOST}:${DOCS_PORT}"
echo
echo "Press Ctrl+C to stop."

cd "${ROOT_DIR}"
exec "${ROOT_DIR}/node_modules/.bin/vitepress" dev docs --host "${BIND_HOST}" --port "${DOCS_PORT}" --strictPort
