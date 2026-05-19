# MCP Tools

Gateway 默认通过 `tools/list` 暴露以下工具。实际可调用范围还会受 API Key、access policy、source 和 allowlist 约束。

| Tool | 用途 |
| --- | --- |
| `data.query_table` | 查询 allowlist 内的 MySQL 表 |
| `redis.query_key` | 只读查询 allowlist 内的 Redis key |
| `kubernetes.list_resources` | 列出 allowlist namespace/resource |
| `kubernetes.get_resource` | 获取单个 Kubernetes 资源摘要 |
| `kubernetes.rollout_status` | 查询 workload rollout 状态和历史 |
| `kubernetes.query_pod_logs` | 查询 allowlist Pod 日志 |
| `logs.query_app_logs` | 从 Redis 日志索引查询应用日志摘要 |
| `audit.query_approval_events` | 查询认证、授权和动作审计事件 |

`kubernetes.kubectl_read` 默认隐藏，只有 `KUBERNETES_ENABLE_RAW_KUBECTL=true` 时才返回。

## `data.query_table`

查询 allowlist 内的 MySQL 表。Gateway 会校验表名、列、过滤字段、limit、预估扫描行数和脱敏规则。

```json
{
  "source_name": "mysql-main",
  "table_name": "orders",
  "columns": ["id", "status", "total"],
  "filters": {
    "status": "paid"
  },
  "limit": 100
}
```

必填字段：

| 字段 | 说明 |
| --- | --- |
| `table_name` | 逻辑表名，必须命中 `tableAllowlist` |

可选字段：

| 字段 | 说明 |
| --- | --- |
| `source_name` | 逻辑 source 名，默认 `default` |
| `columns` | 返回列列表 |
| `filters` | 等值过滤条件 |
| `limit` | 返回行数，1 到 1000，最终受 allowlist 限制 |

## `redis.query_key`

只读查询 Redis key。key 必须完整匹配 `redisKeyAllowlist.keyPattern`。

```json
{
  "source_name": "default",
  "key": "demo:user:1",
  "limit": 20
}
```

必填字段：

| 字段 | 说明 |
| --- | --- |
| `key` | Redis key |

## `kubernetes.list_resources`

列出指定 namespace 中的 allowlist 资源，返回结构化摘要。

```json
{
  "source_name": "default",
  "namespace": "default",
  "resource": "pods",
  "label_selector": "app=api",
  "limit": 50
}
```

必填字段：

| 字段 | 说明 |
| --- | --- |
| `namespace` | Kubernetes namespace |
| `resource` | 资源类型，例如 `pods` 或 `deployments` |

## `kubernetes.get_resource`

获取单个 Kubernetes 资源摘要。

```json
{
  "source_name": "default",
  "namespace": "default",
  "resource": "pods",
  "name": "api-7f6d9"
}
```

## `kubernetes.rollout_status`

查询 Deployment、StatefulSet 或 DaemonSet rollout 状态和历史。

```json
{
  "source_name": "default",
  "namespace": "default",
  "resource": "deployments",
  "name": "api",
  "action": "status"
}
```

`action` 可选 `status` 或 `history`。查询指定 rollout revision 的 history 时可以传 `revision`。

## `kubernetes.query_pod_logs`

查询 allowlist Pod 日志。

```json
{
  "source_name": "default",
  "namespace": "default",
  "pod_name": "api-7f6d9",
  "container": "api",
  "tail_lines": 200
}
```

## `logs.query_app_logs`

从 Redis `app_logs:*` 索引读取应用日志摘要。

```json
{
  "app_name": "billing-api",
  "environment": "prod",
  "trace_id": "trc_paid_summary_001",
  "keyword": "12.00",
  "limit": 20
}
```

## `audit.query_approval_events`

查询 Gateway 审计事件。

```json
{
  "actor_id": "svc-order-api",
  "action_name": "data.query_table",
  "limit": 20
}
```
