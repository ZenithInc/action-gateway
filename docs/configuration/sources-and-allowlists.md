# 配置 Source 和 Allowlist

本页说明如何为已部署的 Action Gateway 配置真实 MySQL、Redis、SLS 或 Kubernetes source。完成后，再用 [agctl](/configuration/agctl/) 给调用方授予权限。

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

## 配置 SLS 日志 source

`logs.query_sls_logs` 使用阿里云 SLS `GetLogsV2` 查询 Logstore。`sourceType: "sls"` 提供区域 endpoint 和 AccessKey；调用时再传入 `project`、`logstore`、时间范围和查询语句。

```json
{
  "id": "src_sls-main_sls",
  "sourceName": "sls-main",
  "sourceType": "sls",
  "displayName": "Main SLS",
  "config": {
    "endpoint": "cn-hangzhou.log.aliyuncs.com",
    "project": "sample-project",
    "logstore": "app-logstore"
  },
  "credential": {
    "accessKeyId": "LTAI...",
    "accessKeySecret": "<secret>",
    "securityToken": "<optional-sts-token>"
  },
  "credentialVersion": 1,
  "enabled": true
}
```

`endpoint`、常用 `project`、常用 `logstore` 和凭证都应集中保存在 source 中。`logs.query_sls_logs` 调用仍需要传入 `project` 和 `logstore`，用于明确本次查询目标并匹配 `<project>/<logstore>` access policy。`credentialVersion` 会进入工具响应和审计摘要。Gateway 不会把 AccessKey 或原始查询结果 payload 写入审计。

### 用 `sls-check` 验证 SLS

发布包和源码中都包含 `sls-check` 诊断 CLI，用于在接入 Gateway 前确认 SLS source 凭证、endpoint、project、logstore 和查询语句能正常响应。正式环境应让它读取 Gateway store 中的 SLS source，避免把同一份配置再复制到 `.env`。

如果 source 的 `config` 已包含 `project` 和 `logstore`，发布包中可以直接运行：

```bash
./sls-check \
  --store-file /etc/action-gateway/gateway-store.json \
  --source-name sls-main \
  --query 'content: "=======createOrderProcess=======data====="' \
  --from 1779852171 \
  --to 1779852172 \
  --line 20 \
  --show-logs
```

如果没有把 `project` 或 `logstore` 写入 source，也可以只把查询目标作为参数传入，凭证仍从 source 读取：

```bash
./sls-check \
  --store-file /etc/action-gateway/gateway-store.json \
  --source-name sls-main \
  --project sample-project \
  --logstore app-logstore \
  --query 'content: "=======createOrderProcess=======data====="' \
  --from 1779852171 \
  --to 1779852172 \
  --line 20 \
  --show-logs
```

源码环境中使用同样的 store/source 方式：

```bash
cargo run --bin sls-check -- \
  --store-file gateway-store.example.json \
  --source-name sls-main \
  --project sample-project \
  --logstore app-logstore \
  --query 'content: "=======createOrderProcess=======data====="' \
  --from 1779852171 \
  --to 1779852172 \
  --line 20 \
  --show-logs
```

`from` 和 `to` 使用 Unix 秒，且必须满足 `from < to`。如果业务日志按北京时间描述，先按 UTC+8 换算时间戳；例如 `2026-05-27 11:22:51` 北京时间对应 `1779852171`。默认响应只返回摘要，调试时加 `--show-logs` 才会输出日志正文。`--env-file` 只保留给项目贡献者做本地临时验证，不是发布包推荐路径。

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
