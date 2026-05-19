#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REDIS_FILE="${ROOT_DIR}/scripts/seed-fake-redis-data.redis"

if [[ "${USE_DOCKER_COMPOSE:-1}" == "1" ]]; then
    REDIS_SERVICE="${REDIS_SERVICE:-redis}"

    docker compose -f "${ROOT_DIR}/docker-compose.yml" exec -T \
        "${REDIS_SERVICE}" \
        redis-cli < "${REDIS_FILE}" > /dev/null
else
    REDIS_HOST="${REDIS_HOST:-127.0.0.1}"
    REDIS_PORT="${REDIS_PORT:-6379}"

    redis-cli --host "${REDIS_HOST}" --port "${REDIS_PORT}" < "${REDIS_FILE}" > /dev/null
fi

echo "Seeded demo Redis data."
