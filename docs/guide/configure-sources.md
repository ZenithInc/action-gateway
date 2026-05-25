# 配置 Source 和 Allowlist

本页说明如何为已部署的 Action Gateway 配置真实 MySQL、Redis、Kubernetes 或日志 Redis source。完成后，再用 [agctl](/guide/agctl) 给调用方授予权限。

## 配置顺序

推荐按下面顺序操作：

1. 从 GitHub Release 下载并部署 Gateway。
2. 准备 `GATEWAY_STORE_FILE` 指向的 store 文件。
3. 配置下游 source credential。
4. 配置 allowlist。
5. 启动或重启 Gateway。
6. 用 `agctl` 创建 Principal、Role、RoleBinding 和 API Key。
7. 用 `tools/list` 和 `tools/call` 验证。

## 准备 store 文件

Gateway store 保存控制面状态、source 配置、allowlist 和审计事件。生产环境应把 store 当作 secret 处理，不要提交到 Git。

最小结构：

```json
{
  "principals": [],
  "apiKeys": [],
  "accessPolicies": [],
  "sources": [],
  "tableAllowlist": [],
  "redisKeyAllowlist": [],
  "kubernetesResourceAllowlist": [],
  "auditEvents": []
}
```

启动 Gateway 时指定它：

```bash
GATEWAY_STORE_FILE=/etc/action-gateway/gateway-store.json /opt/action-gateway/bin/action-gateway
```

手工编辑 JSON store 后需要重启 Gateway。通过 Admin API 或 `agctl` 写入的内容会自动持久化。

## Allowlist 和权限 Manifest 的关系

本页配置的是 source 和 allowlist。它们定义这个 Gateway 实例最多能连接哪些下游系统、最多允许触达哪些表、key、namespace 或 resource。

`agctl` manifest 解决的是另一件事：哪个 Principal 能使用哪些 tool 访问哪些资源。即使 `agctl` 已经授权，如果目标表或 key 没有出现在 allowlist 中，调用仍会返回 `not allowlisted`；反过来，allowlist 已经放行但调用方没有 policy，调用会返回 `unauthorized`。

因此配置顺序通常是先设置 source 和 allowlist，再用 `agctl` 给具体调用方授予更小范围的权限。

## 配置 MySQL source

在 `sources` 中添加或修改 MySQL source：

```json
{
  "id": "src_mysql-main_mysql",
  "sourceName": "mysql-main",
  "sourceType": "mysql",
  "displayName": "Main MySQL",
  "config": {},
  "credential": {
    "url": "mysql://gateway_reader:password@mysql.internal:3306/app_db"
  },
  "credentialVersion": 1,
  "enabled": true
}
```

MySQL credential 支持这些 key：

- `url`
- `connectionUrl`
- `databaseUrl`

建议使用只读数据库账号，并限制网络来源。

## 配置 MySQL table allowlist

`data.query_table` 只会查询 `tableAllowlist` 中启用的表：

```json
{
  "sourceName": "mysql-main",
  "tableName": "orders",
  "columns": ["id", "status", "customer_email", "total", "created_at"],
  "maxLimit": 100,
  "maxEstimatedRows": 10000,
  "maskRules": {
    "customer_email": "email"
  },
  "enabled": true
}
```

调用时：

- `table_name` 必须匹配 `tableName`。
- `columns` 只能选择 allowlist 中的列。
- `filters` 也只能使用 allowlist 中的列。
- `order_by` 排序字段也只能使用 allowlist 中的列，最多 3 个字段，方向为 `asc` 或 `desc`。
- `limit` 不能超过 `maxLimit`。
- 查询执行前会先跑 `EXPLAIN`，预估扫描行数不能超过 `maxEstimatedRows`。

## 配置 Redis source

`redis.query_key` 可以使用 `sourceType: "redis"`：

```json
{
  "id": "src_default_redis",
  "sourceName": "default",
  "sourceType": "redis",
  "displayName": "Default Redis",
  "config": {},
  "credential": {
    "url": "redis://:password@redis.internal:6379/0"
  },
  "credentialVersion": 1,
  "enabled": true
}
```

Redis credential 支持这些 key：

- `url`
- `connectionUrl`
- `redisUrl`

如果 Redis 使用 ACL 用户名，URL 可以写成：

```text
redis://username:password@redis.internal:6379/0
```

## 配置 Redis key allowlist

`redis.query_key` 的 key 必须完整匹配 `keyPattern`：

```json
{
  "sourceName": "default",
  "keyPattern": "orders:[A-Za-z0-9_.:-]+",
  "maxValueBytes": 65536,
  "maxMembers": 100,
  "enabled": true
}
```

`keyPattern` 是正则表达式。建议从非常窄的前缀开始，例如只开放 `orders:debug:*`，不要直接开放 `.*`。

## 配置应用日志 source

`logs.query_app_logs` 读取 Redis 中的 `app_logs:*` 索引。你可以配置专用日志 Redis：

```json
{
  "id": "src_default_logs_redis",
  "sourceName": "default",
  "sourceType": "logs_redis",
  "displayName": "Application Logs Redis",
  "config": {},
  "credential": {
    "url": "redis://:password@logs-redis.internal:6379/0"
  },
  "credentialVersion": 1,
  "enabled": true
}
```

如果没有配置 `logs_redis` source，Gateway 会使用启动时的 `REDIS_URL` client。

## 配置 Kubernetes source

Kubernetes 工具通过 `kubectl` 执行只读查询。运行 Gateway 的机器或容器需要安装 `kubectl`。

使用 kubeconfig 文件路径：

```json
{
  "id": "src_default_kubernetes",
  "sourceName": "default",
  "sourceType": "kubernetes",
  "displayName": "Default Kubernetes",
  "config": {},
  "credential": {
    "kubeconfigPath": "/etc/action-gateway/kubeconfig"
  },
  "credentialVersion": 1,
  "enabled": true
}
```

也可以把 kubeconfig 内容放在 `credential.kubeconfig`。生产环境更推荐挂载文件或使用 secret manager。

## 配置 Kubernetes allowlist

```json
{
  "sourceName": "default",
  "namespace": "default",
  "resource": "pods",
  "actions": ["list", "get", "logs"],
  "maxItems": 100,
  "maxOutputBytes": 65536,
  "maxTailLines": 1000,
  "enabled": true
}
```

常用 action：

| Action | 对应工具 |
| --- | --- |
| `list` | `kubernetes.list_resources` |
| `get` | `kubernetes.get_resource` |
| `logs` | `kubernetes.query_pod_logs` |
| `rollout_status` | `kubernetes.rollout_status` 的 status 查询 |
| `rollout_history` | `kubernetes.rollout_status` 的 history 查询 |
| `raw_read` | `kubernetes.kubectl_read`，默认不暴露 |

## 用 Admin API 更新 source

source 可以通过 Admin API 更新；allowlist 当前仍建议在 store 文件里维护。

```bash
curl -s http://127.0.0.1:8080/admin/sources \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_ADMIN_TOKEN" \
  -d '{
    "sourceName": "mysql-main",
    "sourceType": "mysql",
    "displayName": "Main MySQL",
    "credential": {
      "url": "mysql://gateway_reader:password@mysql.internal:3306/app_db"
    },
    "credentialVersion": 1,
    "enabled": true
  }'
```

生产环境建议给受控自动化系统单独发一个带 `scopes.admin=true` 的 API Key。

## 验证配置

重启 Gateway 后，先确认工具可见：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

再调用目标工具。例如验证 MySQL：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/call",
    "params": {
      "name": "data.query_table",
      "arguments": {
        "source_name": "mysql-main",
        "table_name": "orders",
        "columns": ["id", "status", "total"],
        "limit": 10
      }
    }
  }'
```

如果返回未授权，先检查 access policy；如果返回 not allowlisted，检查对应 allowlist。
