# 核心概念

Action Gateway 的目标是让 Agent 能调用内部排障能力，同时让每一次调用都有清晰边界：谁在调用、能调用什么、能访问哪个数据源、能触达哪些资源、调用后留下什么审计记录。

## 一次请求如何被处理

典型请求路径如下：

```text
MCP Client
  -> Authorization: Bearer <token>
  -> POST /mcp tools/call
  -> Gateway 认证 API Key
  -> Gateway 计算 source、tool、action、resource
  -> Access Policy 授权
  -> Source 和 Allowlist 校验
  -> 执行只读查询
  -> 写入 auditEvents
  -> 返回 MCP tool result
```

实际是否能调用成功，取决于两层边界：

- **Access Policy**：这个 Principal 是否有权限调用某个 tool/action/resource。
- **Allowlist**：这个工具是否被允许触达具体表、key、namespace/resource 或日志索引。

两层都通过才会执行下游查询。

## MCP Endpoint

Gateway 暴露 HTTP JSON-RPC endpoint：

```text
POST /mcp
```

兼容别名：

```text
POST /rpc
```

客户端通常按下面顺序工作：

1. `initialize`：确认协议版本和服务能力。
2. `tools/list`：获取当前身份可见工具。
3. `tools/call`：调用某个工具。

## Principal

`Principal` 是调用 Gateway 的主体。生产环境建议为每个 Agent、服务账号或自动化系统创建独立 Principal。

| 字段 | 说明 |
| --- | --- |
| `type` | `service_account`、`user` 或 `legacy_admin` |
| `status` | `active` 或 `disabled` |
| `metadata` | 业务元数据，例如 owner、service、team |

## API Key

API Key 是生产调用入口，放在 HTTP bearer token 中：

```text
Authorization: Bearer agk_<key_id>_<secret>
```

明文 secret 只在创建时返回一次。Gateway store 中只保存 `secretSalt` 和 `secretHash`。

`RPC_TOKEN` 适合作为首次 bootstrap 管理 token。生产调用应使用 API Key，并通过 `agctl` 给每个调用方创建独立 Principal。

## Source

`source` 表示下游数据源。工具调用可以在 arguments 中传 `source_name`，如果省略则使用 `default`。

常见 source 类型：

| 类型 | 用途 |
| --- | --- |
| `mysql` | 支持 `data.query_table` |
| `redis` | 支持 `redis.query_key` |
| `logs_redis` | 支持 `logs.query_app_logs`，未配置时可回退到默认 Redis client |
| `kubernetes` | 支持 Kubernetes 资源、rollout 和 Pod 日志查询 |

建议按项目和环境独立部署 Gateway，例如 `orders-prod` 和 `orders-staging` 分开部署，而不是在同一个 Gateway 实例里混合管理多个环境。

## Allowlist

Allowlist 定义工具能触达的最小资源边界。

| Allowlist | 保护内容 |
| --- | --- |
| `tableAllowlist` | 表名、列、最大 limit、最大预估扫描行数、脱敏规则 |
| `redisKeyAllowlist` | Redis key 正则、最大返回字节数、最大成员数 |
| `kubernetesResourceAllowlist` | namespace、resource、允许动作、输出上限 |

Allowlist 和 access policy 是不同层面的控制。举例：如果 policy 允许 `svc-order-api` 查询 `orders` 表，但 `tableAllowlist` 没有登记 `orders`，调用仍会失败。

## Allowlist 和 agctl Manifest 的区别

Allowlist 和 `agctl` manifest 是两层门禁，不是同一份配置的两种写法。

Allowlist 定义这个 Gateway 实例最多允许触达哪些下游资源。它在 Gateway store 中维护，例如：

- `tableAllowlist`：允许查询哪些 MySQL 表、哪些列、最大 limit、`EXPLAIN` 阈值和脱敏规则。
- `redisKeyAllowlist`：允许读取哪些 Redis key pattern，以及最大返回字节数和最大成员数。
- `kubernetesResourceAllowlist`：允许访问哪些 namespace、resource 和 action。

`agctl` manifest 定义哪个调用方有权使用哪些工具访问哪些资源。它声明 `Principal`、`Role`、`RoleBinding` 和 `ApiKey`，应用后会编译成 Gateway store 中的 `accessPolicies`。

调用成功必须同时通过这两层：

| Allowlist | agctl policy | 结果 |
| --- | --- | --- |
| 没有放行目标资源 | 已授权调用方 | 返回 `not allowlisted` |
| 已放行目标资源 | 没有授权调用方 | 返回 `unauthorized` |
| 已放行目标资源 | 已授权调用方 | 执行下游只读查询 |

例如 Redis：

- `redisKeyAllowlist.keyPattern = "orders:[A-Za-z0-9_.:-]+"` 表示这个 Gateway 实例最多允许读取 `orders:` 前缀下符合规则的 key。
- `agctl` manifest 里的 `resourceNames: ["orders:*"]` 表示某个 Principal 有权限调用 `redis.query_key` 读取这类 key。

例如 MySQL：

- `tableAllowlist` 登记 `mysql-main.orders` 和允许的列，表示这个 Gateway 实例最多允许查询这些列。
- `agctl` manifest 授权 `svc-order-api` 对 `orders` 执行 `select`，表示这个调用方可以使用 `data.query_table` 查询该表。

这种设计把资源安全上限和调用方权限分开，便于做纵深防御：平台团队可以先限制 Gateway 能碰到的资源，再给不同 Agent 或服务账号分配更小的访问范围。

## Access Policy

Access policy 决定某个 Principal 是否能对某个资源执行某个动作。推荐用 `agctl` 从 `Principal`、`Role`、`RoleBinding` 和 `ApiKey` YAML 生成 policy。

常见动作映射：

| Tool | Action | Resource |
| --- | --- | --- |
| `data.query_table` | `select` | `table` |
| `redis.query_key` | `get` | `redis_key` |
| `kubernetes.list_resources` | `list` | `kubernetes` |
| `kubernetes.get_resource` | `get` | `kubernetes` |
| `kubernetes.rollout_status` | `rollout_status` 或 `rollout_history` | `kubernetes` |
| `kubernetes.query_pod_logs` | `logs` | `kubernetes` |
| `logs.query_app_logs` | `query` | `app_logs` |
| `audit.query_approval_events` | `query` | `audit_events` |

Kubernetes resource name 使用下面的形状：

```text
<namespace>/<resource>/<name-or-*>
```

例如：

```text
default/pods/*
default/deployments/api
```

## Audit Event

Gateway 会追加写入认证、授权、管理变更和工具调用审计事件。你可以通过 `audit.query_approval_events` 查询审计摘要。

`auditEvents` 存在同一个 JSON store 中。生产环境需要规划归档策略，避免单个 store 文件无限增长。
