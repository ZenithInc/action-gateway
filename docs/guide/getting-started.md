# 快速开始

本教程会在本地启动 Action Gateway，完成一次 MCP 初始化、工具发现和 Redis 查询。

## 前置条件

- Rust toolchain，用于运行 `action-gateway`。
- Node.js 和 npm，用于运行本文档站点。
- Docker，可选，用于启动 demo Redis。
- `curl`，用于发送 MCP JSON-RPC 请求。

## 启动 demo stack

在仓库根目录执行：

```bash
cd action-gateway
scripts/start-demo-stack.sh
```

脚本会启动 Redis、写入 demo 数据，并在后台启动 Gateway。默认 MCP endpoint 为：

```text
http://127.0.0.1:8080/mcp
```

如果 8080 已被占用，可以指定端口：

```bash
MCP_PORT=8081 scripts/start-demo-stack.sh
```

## 验证服务状态

```bash
curl -s http://127.0.0.1:8080/healthz
```

也可以运行内置 smoke test：

```bash
scripts/smoke-demo-stack.sh
```

## 初始化 MCP 会话

demo token 默认来自 `ACTION_GATEWAY_MCP_TOKEN` 或 `RPC_TOKEN`，脚本没有显式传入时会使用本地默认值。

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
        "name": "local-docs-client",
        "version": "0.1.0"
      }
    }
  }'
```

## 列出可用工具

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer Xbcd20198$' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```

默认工具包括 `data.query_table`、`redis.query_key`、Kubernetes 查询工具、`logs.query_app_logs` 和 `audit.query_approval_events`。

## 调用 Redis 查询工具

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

## 停止 demo stack

```bash
scripts/start-demo-stack.sh stop
```

如果还要停止 Redis：

```bash
STOP_INFRA=1 scripts/start-demo-stack.sh stop
```
