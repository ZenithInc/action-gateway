# 部署与运维

Action Gateway 本身不依赖控制面数据库。运行时状态由 `GATEWAY_STORE_FILE` 指向的 JSON 文件保存。

## 必要环境变量

| 变量 | 说明 |
| --- | --- |
| `RPC_BIND_ADDR` | Gateway 监听地址，例如 `0.0.0.0:8080` |
| `GATEWAY_STORE_FILE` | JSON store 文件路径 |
| `REDIS_URL` | 默认 Redis 连接地址 |
| `RPC_TOKEN` | 本地或 legacy token |
| `ACTION_GATEWAY_MCP_TOKEN` | 本地 Codex MCP 配置使用的 token；未设置 `RPC_TOKEN` 时会作为 legacy token fallback |
| `GATEWAY_ALLOW_LEGACY_RPC_TOKEN` | 非 loopback 环境是否接受 legacy token |
| `KUBERNETES_ENABLE_RAW_KUBECTL` | 是否暴露 raw kubectl 只读诊断工具 |

生产环境建议使用 API Key 认证，并关闭 legacy token。

## 文件存储

如果 `GATEWAY_STORE_FILE` 指向的文件不存在，Gateway 会创建空 store。本仓库提供了结构参考：

```text
action-gateway/gateway-store.example.json
```

部署时应保证该文件所在目录可写、可备份，并被纳入密钥与配置管理流程。store 中包含 source credential，不应公开。

## Docker Compose

本地 compose 配置包含 Redis 和 Gateway：

```bash
cd action-gateway
RPC_TOKEN=change-me \
GATEWAY_ALLOW_LEGACY_RPC_TOKEN=true \
docker compose --profile gateway up -d redis action-gateway
```

默认 MCP endpoint：

```text
http://127.0.0.1:8080/mcp
```

## 健康检查

```bash
curl -s http://127.0.0.1:8080/healthz
```

## 运维建议

- 通过 `agctl` 管理 Principal、Role、RoleBinding 和 ApiKey，避免手工漂移。
- 为每个生产 Agent 或服务账号分配独立 Principal 和 API Key。
- 对 source credential 使用 secret manager，并限制 store 文件权限。
- 对 `auditEvents` 做定期归档，避免单个 JSON store 无限增长。
- 只在 break-glass 诊断场景开启 `KUBERNETES_ENABLE_RAW_KUBECTL`。
