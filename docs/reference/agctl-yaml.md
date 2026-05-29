# agctl YAML

`agctl` 使用 Kubernetes 风格的多文档 YAML 管理 Gateway 权限。当前支持四种 manifest：

| Kind | 用途 |
| --- | --- |
| `Principal` | 定义调用主体 |
| `Role` | 定义权限规则 |
| `RoleBinding` | 把 Role 绑定到 Principal |
| `ApiKey` | 声明 API Key 创建请求 |

所有 manifest 使用同一个 `apiVersion`：

```yaml
apiVersion: gateway.zenithinc.dev/v1
```

## 通用结构

```yaml
apiVersion: gateway.zenithinc.dev/v1
kind: <Kind>
metadata:
  name: <name>
spec:
  ...
```

`metadata.name` 是稳定 ID，只能包含 ASCII 字母、数字、`.`、`-`、`_`，长度不能超过 96 字节。

## Principal

```yaml
apiVersion: gateway.zenithinc.dev/v1
kind: Principal
metadata:
  name: svc-order-api
spec:
  type: service_account
  displayName: Order API
  status: active
  metadata:
    owner: platform
```

| 字段 | 必填 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `spec.type` | 是 | 无 | `service_account`、`user`、`legacy_admin` |
| `spec.displayName` | 否 | 空 | 展示名 |
| `spec.status` | 否 | `active` | `active` 或 `disabled` |
| `spec.metadata` | 否 | `{}` | JSON/YAML object |

## Role

```yaml
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
```

`scope` 中省略字段表示通配。每条 rule 在 v1 中必须且只能包含一个 tool、verb 和 resource。

支持的 `resources`：

| Resource | 对应能力 |
| --- | --- |
| `table` | `data.query_table` |
| `redis_key` | `redis.query_key` |
| `kubernetes` | Kubernetes 查询工具 |
| `sls_logstore` | `logs.query_sls_logs` |
| `audit_events` | `audit.query_approval_events` |

`resourceNames` 可以使用 `*` 表示通配资源名。

## RoleBinding

```yaml
apiVersion: gateway.zenithinc.dev/v1
kind: RoleBinding
metadata:
  name: svc-order-api-order-db-reader
spec:
  principal: svc-order-api
  role: order-db-reader
```

`principal` 和 `role` 必须引用同一个 YAML 文件中的对象。

## ApiKey

```yaml
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

默认 `agctl apply` 不创建 secret。需要创建 API Key 明文时，显式传 `--create-secrets`。

## 编译规则

`Role` 和 `RoleBinding` 不会作为对象持久化。`agctl apply` 会按以下关系生成 `accessPolicies`：

```text
每个 RoleBinding
  每条 Role rule
    每个 resourceNames 项
      => 一条 accessPolicies 记录
```

字段映射：

| YAML 字段 | Store 字段 |
| --- | --- |
| `RoleBinding.spec.principal` | `principalId` |
| `Role.rule.effect` | `effect` |
| `Role.scope.source` | `sourceName` |
| `Role.rule.tools[0]` | `toolName` |
| `Role.rule.verbs[0]` | `actionName` |
| `Role.rule.resources[0]` | `resourceType` |
| `Role.rule.resourceNames[]` | `resourcePattern` |
