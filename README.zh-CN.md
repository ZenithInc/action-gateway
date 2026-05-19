# Action Gateway

[English](README.md)

[![Deploy Docs](https://github.com/ZenithInc/action-gateway/actions/workflows/deploy-docs.yml/badge.svg)](https://github.com/ZenithInc/action-gateway/actions/workflows/deploy-docs.yml)

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
├── README.md              # 英文 README
└── README.zh-CN.md        # 中文 README
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

仓库已经包含把 VitePress 文档发布到 GitHub Pages 的 GitHub Actions workflow。仓库管理员在 Settings 里一次性启用 **Build and deployment > Source > GitHub Actions** 后，文档站地址是：

```text
https://zenithinc.github.io/action-gateway/
```

每次推送到 `main`，如果改动涉及 `docs/`、`package.json`、`package-lock.json` 或文档部署 workflow，都会触发 GitHub Actions 部署。

在仓库根目录本地运行文档站：

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
