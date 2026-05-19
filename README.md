# Action Gateway

[中文](README.zh-CN.md)

[![Deploy Docs](https://github.com/ZenithInc/action-gateway/actions/workflows/deploy-docs.yml/badge.svg)](https://github.com/ZenithInc/action-gateway/actions/workflows/deploy-docs.yml)

Action Gateway is a controlled MCP gateway that lets agents securely query MySQL, Redis, Kubernetes, logs, and audit data through policy-driven tools.

It exposes an HTTP JSON-RPC MCP endpoint, registers internal capabilities as MCP tools, and keeps identity, authorization, source configuration, allowlists, and audit events in a file-backed JSON store.

## Features

- **MCP over HTTP**: exposes `POST /mcp` for `initialize`, `tools/list`, and `tools/call`.
- **Controlled tools**: provides read-focused tools for MySQL, Redis, Kubernetes, application logs, and audit events.
- **Policy-based access**: uses principals, API keys, access policies, sources, and allowlists to constrain every call.
- **File-backed control plane**: stores gateway state in a JSON file configured by `GATEWAY_STORE_FILE`.
- **GitOps-friendly permissions**: includes `agctl` for applying principals, roles, role bindings, and API keys from YAML manifests.
- **Demo stack**: ships with local Redis demo data and smoke-test scripts for quick validation.

## Client Compatibility

Action Gateway currently has only been tested with Codex as the MCP client. Other MCP-compatible clients should work through the same HTTP JSON-RPC interface, but they have not been verified yet.

## Built-in Tools

| Tool | Purpose |
| --- | --- |
| `data.query_table` | Query allowlisted MySQL tables with an `EXPLAIN` gate before execution. |
| `redis.query_key` | Read allowlisted Redis keys with output limits. |
| `kubernetes.list_resources` | List allowlisted Kubernetes resources. |
| `kubernetes.get_resource` | Read summaries for individual allowlisted Kubernetes resources. |
| `kubernetes.rollout_status` | Inspect Deployment, StatefulSet, and DaemonSet rollout status/history. |
| `kubernetes.query_pod_logs` | Query logs for allowlisted pods. |
| `logs.query_app_logs` | Query application log summaries from Redis log indexes. |
| `audit.query_approval_events` | Query authentication, authorization, and tool-call audit events. |

## Repository Layout

```text
.
├── action-gateway/        # Rust gateway service, agctl CLI, examples, Docker files
├── docs/                  # VitePress documentation source
├── package.json           # Documentation site scripts
├── README.md              # English README
└── README.zh-CN.md        # Chinese README
```

## Quick Start

Prerequisites:

- Rust toolchain
- Docker, optional but recommended for the demo Redis stack
- Node.js and npm, only needed for the documentation site
- `curl`

Start the local demo stack:

```bash
git clone git@github.com:ZenithInc/action-gateway.git
cd action-gateway/action-gateway
scripts/start-demo-stack.sh
```

The default MCP endpoint is:

```text
http://127.0.0.1:8080/mcp
```

Check the service:

```bash
curl -s http://127.0.0.1:8080/healthz
scripts/smoke-demo-stack.sh
```

Stop the demo stack:

```bash
scripts/start-demo-stack.sh stop
```

Stop the demo stack and Redis:

```bash
STOP_INFRA=1 scripts/start-demo-stack.sh stop
```

## Example MCP Calls

Initialize an MCP session:

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer Xbcd20198$' \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
      "protocolVersion": "2025-11-25",
      "capabilities": {},
      "clientInfo": {
        "name": "local-client",
        "version": "0.1.0"
      }
    }
  }'
```

List tools:

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer Xbcd20198$' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```

Read a demo Redis key:

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer Xbcd20198$' \
  -d '{
    "jsonrpc": "2.0",
    "id": 3,
    "method": "tools/call",
    "params": {
      "name": "redis.query_key",
      "arguments": {
        "key": "demo:user:1",
        "limit": 20
      }
    }
  }'
```

## Managing Permissions with agctl

`agctl` applies declarative YAML manifests to Action Gateway through the Admin JSON API. A manifest can define principals, roles, role bindings, and API keys.

```bash
cd action-gateway
cargo run --bin agctl -- apply \
  -f example.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$TOKEN"
```

See [`action-gateway/example.yaml`](action-gateway/example.yaml) and [`action-gateway/AGCTL_YAML_SYNTAX.md`](action-gateway/AGCTL_YAML_SYNTAX.md) for the manifest format.

## Documentation

The repository includes a GitHub Actions workflow for publishing the VitePress documentation to GitHub Pages. After GitHub Pages is enabled once in the repository settings with **Build and deployment > Source > GitHub Actions**, the site is available at:

```text
https://zenithinc.github.io/action-gateway/
```

Every push to `main` that changes `docs/`, `package.json`, `package-lock.json`, or the docs workflow triggers a GitHub Actions deployment.

Run the docs locally from the repository root:

```bash
npm install
npm run docs:dev
```

Build the documentation site:

```bash
npm run docs:build
```

## Security Notes

- Production callers should use Gateway API keys in the `Authorization: Bearer agk_<key_id>_<secret>` format.
- API key secrets are returned only once at creation time; the store keeps only salt/hash material.
- Keep `GATEWAY_STORE_FILE` private because it can contain downstream source credentials.
- Disable legacy token access in production unless it is explicitly needed for a controlled local or break-glass workflow.
- Enable raw kubectl diagnostics only when necessary.

## License

Action Gateway is released under the [MIT License](LICENSE).
