# 文件存储结构

Gateway 通过 `GATEWAY_STORE_FILE` 指定 JSON store 文件。该文件保存控制面状态、source 配置、allowlist 和审计事件。

```bash
GATEWAY_STORE_FILE=./gateway-store.json cargo run
```

结构示例位于：

```text
action-gateway/gateway-store.example.json
```

## 顶层字段

| 字段 | 说明 |
| --- | --- |
| `principals` | 调用主体，例如 service account 或 user |
| `apiKeys` | API Key 记录，只保存 salt/hash，不保存明文 secret |
| `accessPolicies` | 编译后的授权策略 |
| `sources` | 下游 MySQL、Redis、Kubernetes source 配置和 credential |
| `tableAllowlist` | `data.query_table` 可访问表、列、脱敏和 EXPLAIN 阈值 |
| `redisKeyAllowlist` | `redis.query_key` 可访问 key 正则和返回大小限制 |
| `kubernetesResourceAllowlist` | Kubernetes namespace、resource 和 action 白名单 |
| `auditEvents` | Gateway 追加写入的审计事件 |

## Source

```json
{
  "id": "src_mysql-main_mysql",
  "sourceName": "mysql-main",
  "sourceType": "mysql",
  "displayName": "Main MySQL",
  "config": {},
  "credential": {
    "url": "mysql://user:password@mysql-host:3306/app_db"
  },
  "credentialVersion": 1,
  "enabled": true
}
```

`credential` 可能包含数据库连接串、Redis URL 或 kubeconfig 路径。生产 store 应按敏感配置处理。

## Table Allowlist

```json
{
  "sourceName": "mysql-main",
  "tableName": "orders",
  "columns": ["id", "status", "customer_email", "total"],
  "maxLimit": 100,
  "maxEstimatedRows": 10000,
  "maskRules": {
    "customer_email": "email"
  },
  "enabled": true
}
```

`data.query_table` 只允许访问 allowlist 中的表和列。`filters` 也必须使用允许的字段。

## Redis Key Allowlist

```json
{
  "sourceName": "default",
  "keyPattern": "demo:[A-Za-z0-9_.:-]+",
  "maxValueBytes": 65536,
  "maxMembers": 100,
  "enabled": true
}
```

`keyPattern` 使用正则表达式，调用时必须完整匹配。

## Kubernetes Resource Allowlist

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

Kubernetes 工具会同时检查 source、namespace、resource 和 action。
