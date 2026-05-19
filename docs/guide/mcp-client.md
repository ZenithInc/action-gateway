# 接入 MCP Client

Action Gateway 使用 HTTP JSON-RPC 暴露 MCP endpoint。客户端通常按 `initialize`、`tools/list`、`tools/call` 的顺序工作。

## Endpoint 和认证

```text
POST /mcp
```

生产环境使用 Gateway API Key：

```text
Authorization: Bearer agk_<key_id>_<secret>
```

本地 demo 可以使用 legacy token。非 loopback 绑定时，只有显式设置 `GATEWAY_ALLOW_LEGACY_RPC_TOKEN=true` 才接受 legacy token。

## initialize

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
      "protocolVersion": "2025-11-25",
      "capabilities": {},
      "clientInfo": {
        "name": "example-agent",
        "version": "0.1.0"
      }
    }
  }'
```

Gateway 会返回协议版本、服务信息和工具能力声明。

## tools/list

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```

默认会返回当前认证身份可见的工具。`kubernetes.kubectl_read` 默认隐藏，只有设置 `KUBERNETES_ENABLE_RAW_KUBECTL=true` 时才会出现。

## tools/call

工具调用统一使用 `tools/call`：

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "data.query_table",
    "arguments": {}
  }
}
```

响应使用 MCP tool result 形状：

```json
{
  "content": [
    {
      "type": "text",
      "text": "human readable summary"
    }
  ],
  "structuredContent": {},
  "isError": false
}
```

## 指定 source

source-backed tools 可以在 arguments 中传 `source_name`：

```json
{
  "source_name": "mysql-main"
}
```

如果省略 `source_name`，Gateway 使用 `default` source。

## 查询表数据

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{
    "jsonrpc": "2.0",
    "id": 4,
    "method": "tools/call",
    "params": {
      "name": "data.query_table",
      "arguments": {
        "source_name": "mysql-main",
        "table_name": "orders",
        "columns": ["id", "status", "total"],
        "filters": {
          "status": "paid"
        },
        "limit": 10
      }
    }
  }'
```

## 查询应用日志

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{
    "jsonrpc": "2.0",
    "id": 5,
    "method": "tools/call",
    "params": {
      "name": "logs.query_app_logs",
      "arguments": {
        "app_name": "billing-api",
        "environment": "prod",
        "keyword": "12.00",
        "limit": 20
      }
    }
  }'
```

## ping

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{"jsonrpc":"2.0","id":6,"method":"ping"}'
```
