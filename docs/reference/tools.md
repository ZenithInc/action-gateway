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
| `logs.query_sls_logs` | `query` | `sls_logstore` | 查询阿里云 SLS Logstore 日志 |
| `audit.query_approval_events` | `query` | `audit_events` | 查询认证、授权和动作审计事件 |

`kubernetes.kubectl_read` 默认隐藏。只有设置 `KUBERNETES_ENABLE_RAW_KUBECTL=true` 时才会出现在 `tools/list` 中。

## `data.query_table`

查询 allowlist 内的 MySQL 表。Gateway 会校验表名、列、过滤字段、排序字段、limit、预估扫描行数和脱敏规则。

```json
{
  "source_name": "mysql-main",
  "table_name": "orders",
  "columns": ["id", "status", "total"],
  "filters": {
    "status": "paid"
  },
  "order_by": [
    {"column": "created_at", "direction": "desc"}
  ],
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
| `order_by` | 否 | 排序条件数组，最多 3 个字段；每个字段必须在 allowlist 中，`direction` 可为 `asc` 或 `desc`，默认 `asc` |
| `limit` | 否 | 返回行数，最终受 `maxLimit` 限制 |

Policy resource name 使用表名，例如 `orders`。

## `redis.query_key`

只读查询 Redis key。key 必须完整匹配 `redisKeyAllowlist.keyPattern`。

```json
{
  "source_name": "default",
  "key": "orders:123",
  "limit": 20
}
```

字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `key` | 是 | Redis key |
| `source_name` | 否 | Redis source，默认 `default` |
| `limit` | 否 | 集合成员或 entries 返回上限，最终受 `maxMembers` 限制 |

Policy resource name 使用 Redis key，可用通配符，例如 `orders:*`。

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

## `logs.query_sls_logs`

使用阿里云 SLS `GetLogsV2` 查询 Logstore。调用方直接提供 SLS 查询语句或 SQL；Gateway 不解析、不改写查询文本，只校验 source、资源授权、时间范围、长度和分页上限。

```json
{
  "source_name": "sls-main",
  "project": "sample-project",
  "logstore": "app-logstore",
  "from": 1627268185,
  "to": 1627268245,
  "query": "status: 401 | select count(*) as pv",
  "line": 0,
  "offset": 0,
  "reverse": true,
  "power_sql": false
}
```

字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `project` | 是 | SLS project |
| `logstore` | 是 | SLS Logstore |
| `from` | 是 | 查询开始时间，Unix 秒 |
| `to` | 是 | 查询结束时间，Unix 秒，必须大于 `from` |
| `query` | 是 | SLS 查询语句或分析 SQL，最大 16 KiB |
| `source_name` | 否 | SLS source，默认 `default` |
| `line` | 否 | 返回行数，`0..100`，默认 `100`；SQL 分析语句通常用 SQL `LIMIT` 分页 |
| `offset` | 否 | 起始偏移，默认 `0` |
| `reverse` | 否 | 是否按时间倒序返回，默认 `false` |
| `topic` | 否 | SLS topic |
| `power_sql` | 否 | 是否启用 Dedicated SQL，默认 `false` |

Policy resource name 使用 `<project>/<logstore>`，例如 `sample-project/app-logstore`。

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
