# MCP Tools

Gateway 通过 `tools/list` 暴露 MCP tools。实际可调用范围还会受 API Key、access policy、source 和 allowlist 约束。

## 工具总览

| Tool | Action | Resource | 用途 |
| --- | --- | --- | --- |
| `data.query_table` | `select` | `table` | 查询 allowlist 内的 MySQL 表 |
| `redis.query_key` | `get` | `redis_key` | 只读查询 allowlist 内的 Redis key |
| `kubernetes.list_resources` | `list` | `kubernetes` | 列出 allowlist namespace/resource |
| `kubernetes.get_resource` | `get` | `kubernetes` | 获取单个 Kubernetes 资源摘要 |
| `kubernetes.rollout_status` | `rollout_status` / `rollout_history` | `kubernetes` | 查询 workload rollout 状态和历史 |
| `kubernetes.query_pod_logs` | `logs` | `kubernetes` | 查询 allowlist Pod 日志 |
| `logs.query_app_logs` | `query` | `app_logs` | 从 Redis 日志索引查询应用日志摘要 |
| `audit.query_approval_events` | `query` | `audit_events` | 查询认证、授权和动作审计事件 |

`kubernetes.kubectl_read` 默认隐藏。只有设置 `KUBERNETES_ENABLE_RAW_KUBECTL=true` 时才会出现在 `tools/list` 中。

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

字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `table_name` | 是 | 逻辑表名，必须命中 `tableAllowlist` |
| `source_name` | 否 | 逻辑 source 名，默认 `default` |
| `columns` | 否 | 返回列列表，只能使用 allowlist 中的列 |
| `filters` | 否 | 等值过滤条件，字段必须在 allowlist 中 |
| `limit` | 否 | 返回行数，最终受 `maxLimit` 限制 |

Policy resource name 使用表名，例如 `orders`。

## `redis.query_key`

只读查询 Redis key。key 必须完整匹配 `redisKeyAllowlist.keyPattern`。

```json
{
  "source_name": "default",
  "key": "demo:user:1",
  "limit": 20
}
```

字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `key` | 是 | Redis key |
| `source_name` | 否 | Redis source，默认 `default` |
| `limit` | 否 | 集合成员或 entries 返回上限，最终受 `maxMembers` 限制 |

Policy resource name 使用 Redis key，可用通配符，例如 `demo:*`。

## `kubernetes.list_resources`

列出指定 namespace 中的 allowlist 资源，返回结构化摘要。

```json
{
  "source_name": "default",
  "namespace": "default",
  "resource": "pods",
  "label_selector": "app=api",
  "field_selector": "status.phase=Running",
  "limit": 50
}
```

字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `namespace` | 是 | Kubernetes namespace |
| `resource` | 是 | 资源类型，例如 `pods` 或 `deployments` |
| `source_name` | 否 | Kubernetes source，默认 `default` |
| `label_selector` | 否 | Kubernetes label selector |
| `field_selector` | 否 | Kubernetes field selector |
| `limit` | 否 | 返回资源数，最终受 allowlist 限制 |

Policy resource name 形如：

```text
<namespace>/<resource>/*
```

例如 `default/pods/*`。

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

Policy resource name 形如：

```text
<namespace>/<resource>/<name>
```

例如 `default/pods/api-7f6d9`，也可以用 `default/pods/*` 放行一组资源。

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

字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `namespace` | 是 | Kubernetes namespace |
| `resource` | 是 | `deployments`、`statefulsets` 或 `daemonsets` |
| `name` | 是 | workload 名称 |
| `action` | 否 | `status` 或 `history`，默认 `status` |
| `revision` | 否 | 查询指定 rollout revision 的 history |

Policy action：

- `action: "status"` 对应 `rollout_status`
- `action: "history"` 对应 `rollout_history`

## `kubernetes.query_pod_logs`

查询 allowlist Pod 日志。

```json
{
  "source_name": "default",
  "namespace": "default",
  "pod_name": "api-7f6d9",
  "container": "api",
  "tail_lines": 200,
  "since": "10m",
  "previous": false,
  "timestamps": true
}
```

字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `namespace` | 是 | Kubernetes namespace |
| `pod_name` | 是 | Pod 名称 |
| `source_name` | 否 | Kubernetes source，默认 `default` |
| `container` | 否 | 容器名 |
| `tail_lines` | 否 | 返回最后 N 行，最终受 `maxTailLines` 限制 |
| `since` | 否 | Kubernetes duration，例如 `10m`、`1h` |
| `previous` | 否 | 是否查询上一个容器实例日志 |
| `timestamps` | 否 | 是否返回时间戳 |

Policy resource name 使用 `namespace/pods/pod_name`。

## `logs.query_app_logs`

从 Redis `app_logs:*` 索引读取应用日志摘要。

```json
{
  "source_name": "default",
  "app_name": "billing-api",
  "environment": "prod",
  "trace_id": "trc_paid_summary_001",
  "keyword": "12.00",
  "limit": 20
}
```

字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `app_name` | 是 | 应用名 |
| `source_name` | 否 | logs Redis source，默认 `default` |
| `environment` | 否 | 环境名 |
| `trace_id` | 否 | trace id |
| `keyword` | 否 | 文本关键字 |
| `limit` | 否 | 返回条数 |

Policy resource name 使用 `app_name`，例如 `billing-api`。

## `audit.query_approval_events`

查询 Gateway 审计事件。

```json
{
  "actor_id": "svc-order-api",
  "action_name": "data.query_table",
  "limit": 20
}
```

Policy resource name 固定使用：

```text
approval_audit_events
```

## `kubernetes.kubectl_read`

raw kubectl 诊断工具默认隐藏。开启方式：

```bash
KUBERNETES_ENABLE_RAW_KUBECTL=true
```

即使开启，工具仍会限制命令、flag、输出格式、namespace/resource 和输出大小。它适合受控 break-glass 诊断，不建议作为默认 Agent 能力。
