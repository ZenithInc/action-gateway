# action-gateway

`action-gateway` 是 MCP Gateway 服务端 crate。面向使用者时，请优先从 GitHub Release 下载发布产物，而不是 clone 仓库后用 `cargo run` 启动。

仓库内脚本、demo Redis 数据和 fake-order-service 只用于项目开发者本地验证。接入自己的开发、测试或生产环境时，应部署 Release 产物并配置真实 Gateway store。

## 运行时配置

核心环境变量：

| 变量 | 说明 |
| --- | --- |
| `GATEWAY_STORE_FILE` | JSON store 路径，保存 source、allowlist、principal、API key hash、policy 和审计摘要。 |
| `RPC_BIND_ADDR` | 服务监听地址，例如 `0.0.0.0:8080`。 |
| `RPC_TOKEN` | 初始管理 token，用于 bootstrap 管理操作。 |
| `REDIS_URL` | `redis.query_key` 未配置 Redis source 时的默认 Redis client。 |
| `KUBERNETES_ENABLE_RAW_KUBECTL` | 是否暴露 raw kubectl 读取工具，默认关闭。 |

最小启动示例：

```bash
export GATEWAY_STORE_FILE=/etc/action-gateway/gateway-store.json
export RPC_BIND_ADDR=0.0.0.0:8080
export RPC_TOKEN='<replace-with-admin-bootstrap-token>'
export REDIS_URL='redis://:password@redis.internal:6379/0'

/opt/action-gateway/bin/action-gateway
```

## Store

如果 store 文件不存在，Gateway 会创建一个空 JSON store。生产环境建议提前准备好 store，并按 secret 处理。

store 顶层字段：

- `sources`：下游 MySQL、Redis、SLS、Kubernetes source 配置和 credential。
- `tableAllowlist`：`data.query_table` 可访问表、列、脱敏和 `EXPLAIN` 阈值。
- `redisKeyAllowlist`：`redis.query_key` 可访问 key 正则和返回大小限制。
- `kubernetesResourceAllowlist`：Kubernetes namespace、resource 和 action 白名单。
- `principals`、`apiKeys`、`accessPolicies`：调用方身份和授权。
- `auditEvents`：Gateway 审计事件摘要。

真实 source、allowlist 和部署流程见：

- [快速开始](../docs/guide/getting-started.md)
- [配置 Source 和 Allowlist](../docs/guide/configure-sources.md)
- [部署建议](../docs/guide/deployment.md)

## 诊断 CLI

`sls-check` 用于验证 Alibaba Cloud SLS 凭证和 `GetLogsV2` 查询是否能正常响应。它从 `.env` 读取 `AccessKeyID`、`AccessKeySecret`、`SLS_ENDPOINT`、`SLS_PROJECT` 和 `SLS_LOGSTORE`，也支持通过命令行参数覆盖。

```bash
cargo run --bin sls-check -- \
  --env-file ../.env \
  --query 'content: "=======createOrderProcess=======data====="' \
  --from 1779852171 \
  --to 1779852172 \
  --line 20 \
  --show-logs
```

## 开发者 demo

需要开发或调试本仓库时，可以使用 `scripts/start-demo-stack.sh` 启动本地 demo Redis 和 Gateway。该流程不面向普通使用者，也不代表生产部署方式。
