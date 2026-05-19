# 核心概念

Action Gateway 的核心目标是把内部能力暴露给 Agent，同时让身份、授权、数据源、allowlist 和审计保持可解释、可收敛。

## MCP Endpoint

Gateway 暴露 HTTP JSON-RPC 入口：

```text
POST /mcp
```

兼容别名：

```text
POST /rpc
```

客户端通过 `initialize`、`tools/list` 和 `tools/call` 与 Gateway 交互。

## Principal

`Principal` 是调用 Gateway 的主体，可以是服务账号、用户或 legacy admin。生产环境建议为每个服务或 Agent 工作负载创建独立 Principal。

关键字段：

| 字段 | 说明 |
| --- | --- |
| `type` | `service_account`、`user` 或 `legacy_admin` |
| `status` | `active` 或 `disabled` |

## API Key

API Key 是生产调用入口，放在 HTTP bearer token 中：

```text
Authorization: Bearer agk_<key_id>_<secret>
```

明文 secret 只在创建时返回一次。Gateway store 中只保存 `secretSalt` 和 `secretHash`。

## 部署边界

Gateway 不在单个实例里混合管理多个项目或环境。不同项目、不同环境应独立部署 Gateway，因此授权匹配不包含 project/environment 维度。

一次工具调用的边界由 `source_name`、tool、action 和资源名共同决定；未传 `source_name` 时使用 `default`。

## Source

`source` 表示下游数据源。Gateway 自身配置保存在 JSON store，业务数据仍来自下游 MySQL、Redis 或 Kubernetes。

常见 source 类型：

| 类型 | 用途 |
| --- | --- |
| `mysql` | 支持 `data.query_table` |
| `redis` | 支持 `redis.query_key` 和日志索引查询 |
| `kubernetes` | 支持 Kubernetes 资源、rollout 和日志查询 |

## Allowlist

Allowlist 定义工具能触达的最小资源边界。

| Allowlist | 保护内容 |
| --- | --- |
| `tableAllowlist` | 表名、列、过滤字段、最大 limit、最大预估扫描行数、脱敏规则 |
| `redisKeyAllowlist` | Redis key 正则、最大返回字节数、最大成员数 |
| `kubernetesResourceAllowlist` | namespace、resource、允许动作、输出上限 |

## Access Policy

Access policy 决定某个 Principal 是否能对资源执行某个动作。推荐用 `agctl` 从 `Principal`、`Role`、`RoleBinding` 和 `ApiKey` YAML 编译生成 policy。

`Role` 和 `RoleBinding` 本身不持久化，`agctl apply` 会把绑定关系展开成 Gateway store 中的 `accessPolicies`。

## Audit Event

Gateway 会追加写入认证、授权和工具调用审计事件。可以通过 `audit.query_approval_events` 查询审计摘要。
