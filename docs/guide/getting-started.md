# 快速开始

本教程面向准备把 Action Gateway 接入自己开发、测试或生产环境的使用者。推荐从 GitHub Release 下载发布产物，然后配置自己的 source、allowlist 和调用方权限。

本地 demo stack 只适合项目开发者验证示例数据；如果你是使用者，不需要 clone 整个仓库，也不需要启动 fake-order-service。

## 准备条件

| 依赖 | 用途 |
| --- | --- |
| Linux / macOS 主机或容器 | 运行 Gateway |
| 可访问的 MySQL / Redis / Kubernetes | 作为下游数据源 |
| 一个受限的只读账号 | 给 Gateway 查询下游系统 |
| Secret 管理方式 | 保存 store、连接串和 API Key |

## 下载发布产物

从项目的 GitHub Release 页面下载与你的系统匹配的压缩包。发布包通常包含：

- `action-gateway`：MCP Gateway 服务端。
- `agctl`：管理 principal、role、role binding、API key 的 CLI。

示例：

```bash
mkdir -p /opt/action-gateway/bin /etc/action-gateway
tar -xzf action-gateway-<version>-<os>-<arch>.tar.gz -C /opt/action-gateway/bin
chmod +x /opt/action-gateway/bin/action-gateway /opt/action-gateway/bin/agctl
```

如果你的团队使用容器镜像，也可以直接部署对应 Release 的镜像。下面的配置流程相同。

## 创建 store 文件

Gateway 的控制面状态保存在 JSON store 中，包括 source credential、allowlist、principal、API key hash、policy 和审计摘要。

生产环境应把这个文件当作 secret 处理，不要提交到 Git。可以先创建一个最小 store：

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

保存为：

```text
/etc/action-gateway/gateway-store.json
```

## 配置下游 source

在 `sources` 中添加真实环境的连接信息。建议使用只读账号，并限制网络来源。

MySQL source：

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

Redis source：

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

SLS source：

```json
{
  "id": "src_sls-main_sls",
  "sourceName": "sls-main",
  "sourceType": "sls",
  "displayName": "Main SLS",
  "config": {
    "endpoint": "cn-hangzhou.log.aliyuncs.com"
  },
  "credential": {
    "accessKeyId": "LTAI...",
    "accessKeySecret": "<secret>"
  },
  "credentialVersion": 1,
  "enabled": true
}
```

`data.query_table` 使用 `sourceType: "mysql"`；`redis.query_key` 使用 `sourceType: "redis"`；`logs.query_sls_logs` 使用 `sourceType: "sls"`。

## 配置 allowlist

source 只定义“怎么连接”，allowlist 决定“允许查什么”。

MySQL 表白名单：

```json
{
  "sourceName": "mysql-main",
  "tableName": "orders",
  "columns": ["id", "status", "total", "created_at"],
  "maxLimit": 100,
  "maxEstimatedRows": 10000,
  "maskRules": {},
  "enabled": true
}
```

Redis key 白名单：

```json
{
  "sourceName": "default",
  "keyPattern": "orders:[A-Za-z0-9_.:-]+",
  "maxValueBytes": 65536,
  "maxMembers": 100,
  "enabled": true
}
```

`keyPattern` 是完整匹配的正则表达式。生产环境建议从非常窄的业务前缀开始，不要开放 `.*`。

## 启动 Gateway

至少需要指定 store 文件、监听地址和一个管理 token。`REDIS_URL` 只作为 `redis.query_key` 在未配置 Redis source 时的默认 Redis client。

```bash
export GATEWAY_STORE_FILE=/etc/action-gateway/gateway-store.json
export RPC_BIND_ADDR=0.0.0.0:8080
export RPC_TOKEN='<replace-with-admin-bootstrap-token>'
export REDIS_URL='redis://:password@redis.internal:6379/0'

/opt/action-gateway/bin/action-gateway
```

生产环境建议用 systemd、Kubernetes Deployment 或你的进程管理系统托管进程，并把 `RPC_TOKEN`、`REDIS_URL` 和 store 文件放进 Secret 管理系统。

## 创建调用方 API Key

启动后，用 `agctl` 管理调用方身份和权限。下面示例创建一个服务账号，并允许它查询 `mysql-main` source 中的 `orders` 表。

先准备 `order-api-gateway.yaml`：

```yaml
apiVersion: gateway.zenithinc.dev/v1
kind: Principal
metadata:
  name: svc-order-api
spec:
  type: service_account
  displayName: Order API
  status: active
---
apiVersion: gateway.zenithinc.dev/v1
kind: Role
metadata:
  name: order-db-reader
spec:
  scope:
    source: mysql-main
  rules:
    - tools: ["data.query_table"]
      verbs: ["select"]
      resources: ["table"]
      resourceNames:
        - orders
      effect: allow
---
apiVersion: gateway.zenithinc.dev/v1
kind: RoleBinding
metadata:
  name: svc-order-api-order-db-reader
spec:
  principal: svc-order-api
  role: order-db-reader
---
apiVersion: gateway.zenithinc.dev/v1
kind: ApiKey
metadata:
  name: svc-order-api-default
spec:
  principal: svc-order-api
  displayName: Default key
  scopes: {}
  expiresAt: null
```

应用配置并创建 API Key：

```bash
/opt/action-gateway/bin/agctl apply \
  -f order-api-gateway.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$RPC_TOKEN" \
  --create-secrets
```

命令会输出一次明文 token，例如 `agk_<key_id>_<secret>`。立即保存到 secret manager；Gateway store 只保存 salt/hash，不会保存明文。

## 验证

列出当前 API Key 可见的工具：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

查询 allowlist 中的 MySQL 表：

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

如果返回 `unauthorized`，检查 principal、role binding 和 access policy；如果返回 `not allowlisted`，检查对应 source 的 allowlist。

## 下一步

- [配置 Source 和 Allowlist](/guide/configure-sources)：接入更多 MySQL、Redis、SLS 或 Kubernetes。
- [部署建议](/guide/deployment)：把 Gateway 部署到开发、测试或生产环境。
- [接入 MCP Client](/guide/mcp-client)：把 API Key 配置到 Codex 或其他 MCP Client。
