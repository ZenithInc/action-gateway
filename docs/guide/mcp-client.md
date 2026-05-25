# 接入 Codex

Action Gateway 目前只测试过 Codex 作为 MCP client。其他兼容 MCP 的客户端理论上可以通过同一个 HTTP JSON-RPC endpoint 接入，但暂未验证。

## 前置条件

先完成：

1. [快速开始](/guide/getting-started) 或 [部署与运维](/guide/deployment)，确认 Gateway 可用。
2. 准备一个 `agk_<key_id>_<secret>` 格式的 Gateway API Key。

## Codex 配置

在使用 Codex 的项目里添加 `.codex/config.toml`：

```toml
[mcp_servers.action-gateway]
url = "http://127.0.0.1:8080/mcp"
bearer_token_env_var = "ACTION_GATEWAY_API_KEY"
```

如果 Gateway 暴露在内网域名上，可以写成：

```toml
[mcp_servers.action-gateway]
url = "https://gateway.example.com/mcp"
bearer_token_env_var = "ACTION_GATEWAY_API_KEY"
```

启动 Codex 前设置：

```bash
export ACTION_GATEWAY_API_KEY='agk_<key_id>_<secret>'
```

## 验证 Codex 能看到工具

在 Codex 中询问：

```text
List the tools exposed by the action-gateway MCP server.
```

如果配置正确，Codex 应该能看到 Gateway 暴露的工具，例如 `redis.query_key`、`data.query_table` 和 Kubernetes 查询工具。

## 建议给 Codex 的使用方式

让 Codex 先说明它要调用哪个 tool、传什么参数，再执行工具调用。例如：

```text
Use action-gateway to query Redis key orders:123 with limit 20. Show the structured result and explain whether the key is allowlisted.
```

查询 MySQL：

```text
Use action-gateway to query table orders from source mysql-main. Return columns id, status, and total with limit 10.
```

查询 Kubernetes：

```text
Use action-gateway to list pods in namespace default with limit 20.
```

## 直接用 HTTP 验证 MCP

如果 Codex 看不到工具，先绕过 Codex，用 curl 验证 Gateway：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ACTION_GATEWAY_API_KEY" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

这个请求成功但 Codex 看不到工具，通常是 Codex 配置文件位置、变量名或启动环境有问题。

## MCP 方法

Gateway 支持这些 JSON-RPC 方法：

| Method | 用途 |
| --- | --- |
| `initialize` | 返回服务信息和工具能力声明 |
| `notifications/initialized` | MCP 初始化通知，无响应体 |
| `tools/list` | 列出当前身份可见工具 |
| `tools/call` | 调用一个工具 |
| `ping` | 健康探测 |

## 工具调用形状

工具统一通过 `tools/call` 调用：

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "redis.query_key",
    "arguments": {
      "key": "orders:123",
      "limit": 20
    }
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

source-backed tools 可以在 arguments 中传 `source_name`。如果省略，Gateway 使用 `default` source。
