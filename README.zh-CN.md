# Action Gateway

Action Gateway 是一个面向 Agent 的受控 MCP 网关，用 policy 驱动的工具安全暴露 MySQL、Redis、Kubernetes、应用日志和审计查询能力。

它适合放在 Agent 和内部系统之间：Agent 只拿到 Gateway API Key，不直接接触数据库账号、Redis 账号或 kubeconfig。

## 主要能力

- **受控工具集**：提供偏只读的 MySQL、Redis、Kubernetes、应用日志和审计查询能力。
- **Source 隔离**：每个 MySQL、Redis、Kubernetes 或日志 Redis 都通过独立 source 配置。
- **Allowlist 门禁**：MySQL 表、Redis key、Kubernetes namespace/resource/action 都需要显式白名单。
- **身份与授权**：通过 principal、role、role binding、API Key 和 access policy 控制调用范围。
- **审计记录**：记录工具调用摘要，避免把完整业务数据、日志正文或 Redis 值写入审计。

## 用户如何开始

面向使用者的推荐路径是：

1. 从 GitHub Release 下载与你的系统匹配的发布包，或使用对应版本的容器镜像。
2. 准备 Gateway store，并把它当作 secret 管理。
3. 在 store 中配置真实 MySQL、Redis、Kubernetes 或日志 Redis source。
4. 配置 `tableAllowlist`、`redisKeyAllowlist` 或 `kubernetesResourceAllowlist`。
5. 启动 `action-gateway`。
6. 用 `agctl` 给调用方创建 principal、role binding 和 API Key。
7. 在 Codex 或其他 MCP Client 中配置 Gateway endpoint 和 API Key。

完整步骤见文档：[快速开始](docs/guide/getting-started.md)。

仓库里的 demo stack 只用于项目开发者本地验证示例流程。接入自己的开发、测试或生产环境时，不需要 clone 整个仓库，也不需要启动 fake-order-service。

## 最小配置示例

创建 `/etc/action-gateway/gateway-store.json`：

```json
{
  "principals": [],
  "apiKeys": [],
  "accessPolicies": [],
  "sources": [
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
    },
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
  ],
  "tableAllowlist": [
    {
      "sourceName": "mysql-main",
      "tableName": "orders",
      "columns": ["id", "status", "total", "created_at"],
      "maxLimit": 100,
      "maxEstimatedRows": 10000,
      "maskRules": {},
      "enabled": true
    }
  ],
  "redisKeyAllowlist": [
    {
      "sourceName": "default",
      "keyPattern": "orders:[A-Za-z0-9_.:-]+",
      "maxValueBytes": 65536,
      "maxMembers": 100,
      "enabled": true
    }
  ],
  "kubernetesResourceAllowlist": [],
  "auditEvents": []
}
```

启动：

```bash
export GATEWAY_STORE_FILE=/etc/action-gateway/gateway-store.json
export RPC_BIND_ADDR=0.0.0.0:8080
export RPC_TOKEN='<replace-with-admin-bootstrap-token>'
export REDIS_URL='redis://:password@redis.internal:6379/0'

/opt/action-gateway/bin/action-gateway
```

## 工具

| Tool | 说明 |
| --- | --- |
| `data.query_table` | 查询 allowlist 内的 MySQL 表，并在执行前通过 `EXPLAIN` 门禁。 |
| `redis.query_key` | 只读查询 allowlist 内的 Redis key，并限制输出大小。 |
| `kubernetes.list_resources` | 查询 allowlist 内的 Kubernetes 资源列表。 |
| `kubernetes.get_resource` | 查询 allowlist 内的 Kubernetes 单个资源。 |
| `kubernetes.query_pod_logs` | 查询 allowlist 内的 Pod 日志。 |
| `kubernetes.rollout_status` | 查询 Deployment / StatefulSet / DaemonSet rollout 状态或历史。 |
| `logs.query_app_logs` | 从 Redis 日志索引查询应用日志摘要。 |
| `audit.query_events` | 查询 Gateway 审计事件摘要。 |

## 文档

- [快速开始](docs/guide/getting-started.md)
- [配置 Source 和 Allowlist](docs/guide/configure-sources.md)
- [部署建议](docs/guide/deployment.md)
- [接入 MCP Client](docs/guide/mcp-client.md)
- [Store 结构](docs/reference/store.md)
