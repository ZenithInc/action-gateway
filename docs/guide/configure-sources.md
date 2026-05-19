# 配置 Source 和 Allowlist

本页说明如何把本地 demo 配置替换成真实 MySQL、Redis 或 Kubernetes。完成后，再用 [agctl](/guide/agctl) 给调用方授予权限。

## 配置顺序

推荐按下面顺序操作：

1. 为当前项目和环境准备一个 Gateway 实例。
2. 准备 `GATEWAY_STORE_FILE`。
3. 配置下游 source credential。
4. 配置 allowlist。
5. 启动或重启 Gateway。
6. 用 `agctl` 创建 Principal、Role、RoleBinding 和 API Key。
7. 用 `tools/list` 和 `tools/call` 验证。

## 准备 store 文件

从示例复制一份：

```bash
cd action-gateway
cp gateway-store.example.json .local/run/gateway-store.json
```

启动 Gateway 时指定它：

```bash
GATEWAY_STORE_FILE=.local/run/gateway-store.json cargo run
```

如果手工编辑 JSON store，需要重启 Gateway。Gateway 启动时读取 store，运行期间通过 Admin API 或 `agctl` 写入的内容会自动持久化。

::: warning
store 里可能包含数据库连接串、Redis URL、kubeconfig 或 API key hash。不要把真实生产 store 提交到 Git。
:::

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
    "url": "mysql://gateway_reader:password@mysql.example.com:3306/app_db"
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
    "url": "redis://redis.example.com:6379/"
  },
  "credentialVersion": 1,
  "enabled": true
}
```

Redis credential 支持这些 key：

- `url`
- `connectionUrl`
- `redisUrl`

## 配置 Redis key allowlist

`redis.query_key` 的 key 必须完整匹配 `keyPattern`：

```json
{
  "sourceName": "default",
  "keyPattern": "demo:[A-Za-z0-9_.:-]+",
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
    "url": "redis://logs-redis.example.com:6379/"
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

本地 demo 中可以先把 legacy token 作为 admin token：

```bash
export GATEWAY_ADMIN_TOKEN="$ACTION_GATEWAY_MCP_TOKEN"
```

示例：

```bash
curl -s http://127.0.0.1:8080/admin/sources \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_ADMIN_TOKEN" \
  -d '{
    "sourceName": "mysql-main",
    "sourceType": "mysql",
    "displayName": "Main MySQL",
    "credential": {
      "url": "mysql://gateway_reader:password@mysql.example.com:3306/app_db"
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
