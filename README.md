# Action Gateway

[English](#english) | [中文](#中文)

## English

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
└── README.md
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

Run the documentation site locally from the repository root:

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

---

## 中文

Action Gateway 是一个面向 Agent 的受控 MCP 网关，用 policy 驱动的工具安全暴露 MySQL、Redis、Kubernetes、日志和审计查询能力。

它通过 HTTP JSON-RPC 暴露 MCP endpoint，把内部能力注册为 MCP tools，并使用文件化 JSON store 保存身份、授权、数据源、allowlist 和审计事件。

## 功能特性

- **HTTP MCP 接口**：通过 `POST /mcp` 支持 `initialize`、`tools/list` 和 `tools/call`。
- **受控工具集**：提供偏只读的 MySQL、Redis、Kubernetes、应用日志和审计查询能力。
- **基于策略的访问控制**：用 principal、API key、access policy、source 和 allowlist 约束每一次调用。
- **文件化控制面**：Gateway 状态保存在 `GATEWAY_STORE_FILE` 指定的 JSON 文件中。
- **适合 GitOps 的权限管理**：提供 `agctl`，可从 YAML manifest 应用 principal、role、role binding 和 API key。
- **本地 demo stack**：内置 Redis demo 数据和 smoke test 脚本，便于快速验证。

## 客户端兼容性

Action Gateway 目前只测试过 Codex 作为 MCP client。其他兼容 MCP 的客户端理论上可以通过同一个 HTTP JSON-RPC 接口接入，但暂未验证。

## 内置工具

| Tool | 用途 |
| --- | --- |
| `data.query_table` | 查询 allowlist 内的 MySQL 表，并在执行前通过 `EXPLAIN` 门禁。 |
| `redis.query_key` | 只读查询 allowlist 内的 Redis key，并限制输出大小。 |
| `kubernetes.list_resources` | 列出 allowlist 内的 Kubernetes 资源。 |
| `kubernetes.get_resource` | 查询单个 allowlist Kubernetes 资源摘要。 |
| `kubernetes.rollout_status` | 查询 Deployment、StatefulSet 和 DaemonSet rollout 状态/历史。 |
| `kubernetes.query_pod_logs` | 查询 allowlist Pod 日志。 |
| `logs.query_app_logs` | 从 Redis 日志索引查询应用日志摘要。 |
| `audit.query_approval_events` | 查询认证、授权和工具调用审计事件。 |

## 仓库结构

```text
.
├── action-gateway/        # Rust Gateway 服务、agctl CLI、示例和 Docker 文件
├── docs/                  # VitePress 文档源码
├── package.json           # 文档站脚本
└── README.md
```

## 快速开始

前置条件：

- Rust toolchain
- Docker，可选，但推荐用于 demo Redis stack
- Node.js 和 npm，仅运行文档站时需要
- `curl`

启动本地 demo stack：

```bash
git clone git@github.com:ZenithInc/action-gateway.git
cd action-gateway/action-gateway
scripts/start-demo-stack.sh
```

默认 MCP endpoint：

```text
http://127.0.0.1:8080/mcp
```

检查服务状态：

```bash
curl -s http://127.0.0.1:8080/healthz
scripts/smoke-demo-stack.sh
```

停止 demo stack：

```bash
scripts/start-demo-stack.sh stop
```

同时停止 demo stack 和 Redis：

```bash
STOP_INFRA=1 scripts/start-demo-stack.sh stop
```

## MCP 调用示例

初始化 MCP 会话：

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

列出工具：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer Xbcd20198$' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```

读取 demo Redis key：

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

## 使用 agctl 管理权限

`agctl` 通过 Admin JSON API 把声明式 YAML manifest 应用到 Action Gateway。一个 manifest 可以定义 principal、role、role binding 和 API key。

```bash
cd action-gateway
cargo run --bin agctl -- apply \
  -f example.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$TOKEN"
```

manifest 格式可以参考 [`action-gateway/example.yaml`](action-gateway/example.yaml) 和 [`action-gateway/AGCTL_YAML_SYNTAX.md`](action-gateway/AGCTL_YAML_SYNTAX.md)。

## 文档

在仓库根目录运行文档站：

```bash
npm install
npm run docs:dev
```

构建文档站：

```bash
npm run docs:build
```

## 安全提示

- 生产调用方应使用 `Authorization: Bearer agk_<key_id>_<secret>` 格式的 Gateway API key。
- API key 明文 secret 只在创建时返回一次；store 中只保存 salt/hash。
- `GATEWAY_STORE_FILE` 可能包含下游 source credential，应保持私密。
- 生产环境不建议开启 legacy token，除非用于受控的本地或 break-glass 流程。
- raw kubectl 诊断能力只应在必要时开启。

## 开源协议

Action Gateway 基于 [MIT License](LICENSE) 开源。
