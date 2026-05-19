# 快速开始

本教程会从一个全新的 clone 开始，在本地启动 Action Gateway，验证 MCP endpoint，并完成一次 Redis 查询。跑通后，你就可以继续接入 Codex 或替换成真实数据源。

## 前置条件

本地需要安装：

| 工具 | 用途 |
| --- | --- |
| Rust toolchain | 编译并运行 Gateway 和 `agctl` |
| Docker | 启动 demo Redis |
| Node.js | demo 脚本用于校验 MCP tools；文档站也需要它 |
| `curl` | 发送 MCP JSON-RPC 请求 |

## 获取代码

```bash
git clone git@github.com:ZenithInc/action-gateway.git
cd action-gateway/action-gateway
```

## 启动 demo stack

```bash
scripts/start-demo-stack.sh
```

脚本会完成这些工作：

- 启动一个 Docker Redis。
- 复制 `gateway-store.example.json` 到本地 store。
- 写入 demo Redis 数据。
- 在后台启动 Action Gateway。
- 打印 MCP endpoint、Admin API 地址、store 文件位置和日志目录。

默认 MCP endpoint 是：

```text
http://127.0.0.1:8080/mcp
```

如果 8080 已被占用，可以指定端口：

```bash
MCP_PORT=8081 scripts/start-demo-stack.sh
```

## 验证服务

健康检查：

```bash
curl -s http://127.0.0.1:8080/healthz
```

运行内置 smoke test：

```bash
scripts/smoke-demo-stack.sh
```

查看当前状态：

```bash
scripts/start-demo-stack.sh status
```

## 设置本地 token

demo stack 默认使用 `ACTION_GATEWAY_MCP_TOKEN` 或 `RPC_TOKEN`。如果你没有显式设置，脚本会创建并复用 `.local/run/action-gateway-token`：

```bash
export ACTION_GATEWAY_MCP_TOKEN="$(cat .local/run/action-gateway-token)"
```

这个 legacy token 只适合本地 demo。生产环境应使用 Gateway API Key，详见 [使用 agctl 管理权限](/guide/agctl)。

## 初始化 MCP 会话

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ACTION_GATEWAY_MCP_TOKEN" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
      "protocolVersion": "2025-11-25",
      "capabilities": {},
      "clientInfo": {
        "name": "local-docs-client",
        "version": "0.1.0"
      }
    }
  }'
```

如果返回 `serverInfo` 和 `capabilities.tools`，说明 MCP endpoint 已可用。

## 列出工具

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ACTION_GATEWAY_MCP_TOKEN" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```

你应该能看到这些工具：

- `data.query_table`
- `redis.query_key`
- `kubernetes.list_resources`
- `kubernetes.get_resource`
- `kubernetes.rollout_status`
- `kubernetes.query_pod_logs`
- `logs.query_app_logs`
- `audit.query_approval_events`

## 查询 demo Redis key

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ACTION_GATEWAY_MCP_TOKEN" \
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

成功响应会包含 `content` 和 `structuredContent`。如果返回 `isError: true`，通常是 key 没有命中 allowlist、token 不正确，或 Redis 没有启动。

## 下一步

本地 demo 跑通后，继续做真实接入：

1. 按 [配置 Source 和 Allowlist](/guide/configure-sources) 把 demo Redis 换成你的 MySQL、Redis 或 Kubernetes。
2. 按 [使用 agctl 管理权限](/guide/agctl) 创建 Principal、Role、RoleBinding 和 API Key。
3. 按 [接入 Codex](/guide/mcp-client) 把 Gateway 配进 Codex。

## 停止 demo stack

只停止 Gateway：

```bash
scripts/start-demo-stack.sh stop
```

同时停止 Gateway 和 Redis：

```bash
STOP_INFRA=1 scripts/start-demo-stack.sh stop
```
