# MCP Client Schema

本文档描述 `action-gateway-v2` 通过 MCP JSON-RPC 暴露给 MCP Client 的协议入口、服务能力和工具输入 schema。Schema 来源为 `src/mcp.rs`、`src/actions.rs` 和 `src/audit.rs`。

## Endpoint

- Primary endpoint: `POST /mcp`
- Compatibility alias: `POST /rpc`
- Transport payload: JSON-RPC 2.0
- Auth: `Authorization: Bearer agk_<key_id>_<secret>` for Gateway API keys. Local/demo may still use `Authorization: Bearer <RPC_TOKEN>` or `<ACTION_GATEWAY_MCP_TOKEN>` when legacy compatibility is enabled.
- Default protocol version: `2025-11-25`

当 `RPC_BIND_ADDR` 不是 loopback 地址时，缺失 bearer token 的请求会被拒绝。旧 `RPC_TOKEN`/`ACTION_GATEWAY_MCP_TOKEN` 只在 loopback 或 `GATEWAY_ALLOW_LEGACY_RPC_TOKEN=true` 时接受。

## JSON-RPC Methods

| Method | Description |
| --- | --- |
| `initialize` | Returns server metadata, protocol version, and tool capability metadata. |
| `notifications/initialized` | Notification-only method. No response body. |
| `tools/list` | Returns the MCP tools below. |
| `tools/call` | Invokes one MCP tool by `params.name` and `params.arguments`. |
| `ping` | Returns an empty result object. |

## Initialize Result

```json
{
  "protocolVersion": "2025-11-25",
  "capabilities": {
    "tools": {
      "listChanged": false
    }
  },
  "serverInfo": {
    "name": "action-skills-mcp-gateway",
    "title": "Action Skills MCP Gateway",
    "version": "0.1.0",
    "description": "Gateway exposing operational actions as MCP tools."
  },
  "instructions": "Use tools/list to discover the actions visible to the authenticated Gateway API key and tools/call to invoke them. Source-backed tools accept an optional source_name. Data table queries use a registered MySQL source and require table policy. Redis key queries are read-only and require key policy. Kubernetes access is structured-tool-first and constrained by source, namespace, resource, and action policy; raw kubectl is hidden unless KUBERNETES_ENABLE_RAW_KUBECTL=true and should be reserved for break-glass diagnostics. Application log queries read bounded summaries from registered Redis log indexes."
}
```

## Tool Call Envelope

MCP clients call tools through `tools/call`.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "data.query_table",
    "arguments": {}
  }
}
```

Tool responses use the MCP tool result shape:

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

For source-backed tools, `source_name` identifies the data-source scope. If omitted, Gateway uses `default`.

## Default Tools

These tools are returned by `tools/list` by default.

| Tool | Title |
| --- | --- |
| `data.query_table` | Query Table Data |
| `redis.query_key` | Query Redis Key |
| `kubernetes.list_resources` | List Kubernetes Resources |
| `kubernetes.get_resource` | Get Kubernetes Resource |
| `kubernetes.rollout_status` | Kubernetes Rollout Status |
| `kubernetes.query_pod_logs` | Query Pod Logs |
| `logs.query_app_logs` | Query Application Logs |
| `audit.query_approval_events` | Query Approval Audit Events |

`kubernetes.kubectl_read` is hidden by default and is only returned when `KUBERNETES_ENABLE_RAW_KUBECTL=true`.

## `data.query_table`

Query rows from an allowlisted MySQL table after passing an `EXPLAIN` gate and applying configured field masking.

```json
{
  "type": "object",
  "properties": {
    "source_name": {
      "type": "string",
      "description": "Optional logical source name in the allowlist.",
      "default": "default"
    },
    "table_name": {
      "type": "string",
      "description": "Logical table name to query."
    },
    "columns": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "description": "Optional list of columns to return."
    },
    "filters": {
      "type": "object",
      "description": "Optional equality filters keyed by column name."
    },
    "limit": {
      "type": "integer",
      "minimum": 1,
      "maximum": 1000,
      "default": 100
    }
  },
  "required": [
    "table_name"
  ],
  "additionalProperties": false
}
```

Runtime validation also requires valid MySQL identifiers, allowlisted columns and filters, scalar filter values, configured `max_limit`, configured `max_estimated_rows`, and valid `mask_rules`.

## `redis.query_key`

Read a Redis key after matching it against the configured key allowlist. This tool only runs read commands.

```json
{
  "type": "object",
  "properties": {
    "source_name": {
      "type": "string",
      "description": "Optional logical Redis source name in the allowlist.",
      "default": "default"
    },
    "key": {
      "type": "string",
      "description": "Redis key to query."
    },
    "limit": {
      "type": "integer",
      "minimum": 1,
      "maximum": 1000,
      "description": "Maximum collection members or entries to return."
    }
  },
  "required": [
    "key"
  ],
  "additionalProperties": false
}
```

Runtime validation also requires the key to match a full `redisKeyAllowlist.keyPattern` entry in the Gateway store file. Returned values are bounded by `maxValueBytes` and `maxMembers`.

## `kubernetes.list_resources`

List allowlisted Kubernetes resources in one namespace. Returns structured summaries, not raw YAML or JSON.

```json
{
  "type": "object",
  "properties": {
    "source_name": {
      "type": "string",
      "description": "Optional logical Kubernetes source name in the allowlist.",
      "default": "default"
    },
    "namespace": {
      "type": "string",
      "description": "Kubernetes namespace."
    },
    "resource": {
      "type": "string",
      "description": "Kubernetes resource type, such as pods or deployments."
    },
    "label_selector": {
      "type": "string",
      "description": "Optional label selector, such as app=api,tier!=debug."
    },
    "field_selector": {
      "type": "string",
      "description": "Optional field selector, such as status.phase=Running."
    },
    "limit": {
      "type": "integer",
      "minimum": 1,
      "maximum": 1000,
      "description": "Maximum resources to return, capped by the allowlist."
    }
  },
  "required": [
    "namespace",
    "resource"
  ],
  "additionalProperties": false
}
```

Runtime validation also requires a matching `kubernetesResourceAllowlist` entry in the Gateway store file with action `list`.

## `kubernetes.get_resource`

Get one allowlisted Kubernetes resource and return a redacted, type-aware status summary.

```json
{
  "type": "object",
  "properties": {
    "source_name": {
      "type": "string",
      "description": "Optional logical Kubernetes source name in the allowlist.",
      "default": "default"
    },
    "namespace": {
      "type": "string",
      "description": "Kubernetes namespace."
    },
    "resource": {
      "type": "string",
      "description": "Kubernetes resource type, such as pods or deployments."
    },
    "name": {
      "type": "string",
      "description": "Resource name."
    }
  },
  "required": [
    "namespace",
    "resource",
    "name"
  ],
  "additionalProperties": false
}
```

Runtime validation also requires a matching `kubernetesResourceAllowlist` entry in the Gateway store file with action `get`.

## `kubernetes.rollout_status`

Query rollout status or history for allowlisted deployments, statefulsets, or daemonsets.

```json
{
  "type": "object",
  "properties": {
    "source_name": {
      "type": "string",
      "description": "Optional logical Kubernetes source name in the allowlist.",
      "default": "default"
    },
    "namespace": {
      "type": "string",
      "description": "Kubernetes namespace."
    },
    "resource": {
      "type": "string",
      "description": "deployments, statefulsets, or daemonsets."
    },
    "name": {
      "type": "string",
      "description": "Workload name."
    },
    "action": {
      "type": "string",
      "enum": [
        "status",
        "history"
      ],
      "default": "status",
      "description": "Rollout query type."
    },
    "revision": {
      "type": "integer",
      "minimum": 1,
      "maximum": 1000000,
      "description": "Optional revision for rollout history."
    }
  },
  "required": [
    "namespace",
    "resource",
    "name"
  ],
  "additionalProperties": false
}
```

Runtime validation also requires `resource` to be `deployments`, `statefulsets`, or `daemonsets`, and a matching allowlist action of `rollout_status` or `rollout_history`.

## `kubernetes.query_pod_logs`

Query allowlisted Kubernetes Pod logs through `kubectl logs`. Tail lines and output bytes are capped by policy.

```json
{
  "type": "object",
  "properties": {
    "source_name": {
      "type": "string",
      "description": "Optional logical Kubernetes source name in the allowlist.",
      "default": "default"
    },
    "namespace": {
      "type": "string",
      "description": "Kubernetes namespace."
    },
    "pod_name": {
      "type": "string",
      "description": "Pod name."
    },
    "container": {
      "type": "string",
      "description": "Optional container name."
    },
    "since": {
      "type": "string",
      "description": "Optional time window, such as 15m or 1h."
    },
    "previous": {
      "type": "boolean",
      "default": false,
      "description": "Return logs for the previous terminated container instance."
    },
    "timestamps": {
      "type": "boolean",
      "default": false,
      "description": "Include timestamps in log lines."
    },
    "tail_lines": {
      "type": "integer",
      "minimum": 1,
      "maximum": 5000,
      "default": 200
    },
    "timeout_seconds": {
      "type": "integer",
      "minimum": 1,
      "maximum": 60,
      "default": 10
    },
    "max_output_bytes": {
      "type": "integer",
      "minimum": 1024,
      "maximum": 1048576,
      "default": 65536
    }
  },
  "required": [
    "namespace",
    "pod_name"
  ],
  "additionalProperties": false
}
```

Runtime validation also requires a matching `kubernetesResourceAllowlist` entry in the Gateway store file for resource `pods` and action `logs`. `tail_lines` and `max_output_bytes` must not exceed the policy entry.

## `logs.query_app_logs`

Query bounded application log summaries from Redis app log indexes by app, environment, trace id, keyword, or recent time window.

```json
{
  "type": "object",
  "properties": {
    "app_name": {
      "type": "string",
      "description": "Application/service name."
    },
    "environment": {
      "type": "string",
      "description": "Optional runtime environment, such as prod or staging."
    },
    "trace_id": {
      "type": "string",
      "description": "Optional trace id."
    },
    "keyword": {
      "type": "string",
      "description": "Optional keyword to search for."
    },
    "since": {
      "type": "string",
      "description": "Optional time window, such as 15m or 1h."
    },
    "limit": {
      "type": "integer",
      "minimum": 1,
      "maximum": 200,
      "default": 50
    }
  },
  "required": [
    "app_name"
  ],
  "additionalProperties": false
}
```

Runtime validation also requires valid app/environment names and an existing Redis log index key. Without `environment`, the index is `app_logs:index:app:<app_name>`. With `environment`, the index is `app_logs:index:app_env:<app_name>:<environment>`.

## `audit.query_approval_events`

Query approval and action audit events captured by the gateway. Returned records are summaries and do not include full business rows, logs, stdout, or Redis values.

```json
{
  "type": "object",
  "properties": {
    "request_id": {
      "type": "string",
      "description": "Filter by gateway request id."
    },
    "approval_id": {
      "type": "string",
      "description": "Filter by approval id when provided by the caller."
    },
    "action_request_id": {
      "type": "string",
      "description": "Filter by action request id when provided by the caller."
    },
    "event_type": {
      "type": "string",
      "description": "Filter by event type, such as action.tool_call."
    },
    "action_name": {
      "type": "string",
      "description": "Filter by MCP tool/action name."
    },
    "actor_id": {
      "type": "string",
      "description": "Filter by actor id captured from headers."
    },
    "after_status": {
      "type": "string",
      "description": "Filter by resulting status."
    },
    "decision": {
      "type": "string",
      "description": "Filter by normalized decision, such as allowed, rejected, or failed."
    },
    "limit": {
      "type": "integer",
      "minimum": 1,
      "maximum": 1000,
      "default": 100
    }
  },
  "additionalProperties": false
}
```

Runtime validation caps filter strings at 255 bytes and `limit` at `1..=1000`.

## Conditional Tool: `kubernetes.kubectl_read`

This tool is only returned by `tools/list` when `KUBERNETES_ENABLE_RAW_KUBECTL=true`.

Advanced diagnostic escape hatch. Disabled unless `KUBERNETES_ENABLE_RAW_KUBECTL=true` and still constrained by Kubernetes allowlist policy.

```json
{
  "type": "object",
  "properties": {
    "source_name": {
      "type": "string",
      "description": "Optional logical Kubernetes source name in the allowlist.",
      "default": "default"
    },
    "args": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "minItems": 1,
      "description": "Arguments after kubectl. Only limited diagnostics are allowed."
    },
    "timeout_seconds": {
      "type": "integer",
      "minimum": 1,
      "maximum": 60,
      "default": 10
    },
    "max_output_bytes": {
      "type": "integer",
      "minimum": 1024,
      "maximum": 1048576,
      "default": 65536,
      "description": "Maximum bytes captured per output stream, capped by policy for resource commands."
    }
  },
  "required": [
    "args"
  ],
  "additionalProperties": false
}
```

Runtime validation only allows limited diagnostic `kubectl` commands: `get`, `describe`, `api-resources`, `api-versions`, `version`, `config current-context`, and `rollout status/history`. Resource-scoped commands require namespace and matching `kubernetesResourceAllowlist` policy in the Gateway store file. Dangerous flags such as all-namespaces, watch, kubeconfig, token, context, raw, server, user, and file flags are rejected.

## Audit Context Headers

Every authorized `tools/call` attempts to write an audit event. Clients may provide these optional headers to enrich audit records:

| Header | Purpose |
| --- | --- |
| `X-Request-Id` | External request id. Generated when absent. |
| `X-Approval-Id` | External approval id. |
| `X-Action-Request-Id` | External action request id. |
| `X-Actor-Id` or `X-User-Id` | Actor id. |
| `X-Actor-Role` | Actor role. |
| `X-Service-Name` | Calling service name. |
| `X-Forwarded-For` or `X-Real-Ip` | Source IP. |

Audit summaries intentionally exclude full MySQL rows, Redis values, raw Kubernetes stdout/stderr, and full log payloads.
