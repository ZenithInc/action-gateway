# 使用 agctl 管理权限

`agctl` 是 Action Gateway 推荐的权限管理方式。你可以把 Principal、Role、RoleBinding 和 ApiKey 写成多文档 YAML，提交到 Git，再通过 Admin JSON API 应用到 Gateway。

## 编写 manifest

最小示例：

```yaml
apiVersion: gateway.youse.dev/v1
kind: Principal
metadata:
  name: svc-order-api
spec:
  type: service_account
  displayName: Order API
---
apiVersion: gateway.youse.dev/v1
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
apiVersion: gateway.youse.dev/v1
kind: RoleBinding
metadata:
  name: svc-order-api-order-db-reader
spec:
  principal: svc-order-api
  role: order-db-reader
---
apiVersion: gateway.youse.dev/v1
kind: ApiKey
metadata:
  name: svc-order-api-default
spec:
  principal: svc-order-api
  displayName: Default key
  scopes: {}
  expiresAt: null
```

仓库内的完整示例位于：

```text
action-gateway/example.yaml
```

## 应用配置

```bash
cd action-gateway
cargo run --bin agctl -- apply \
  -f example.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$TOKEN"
```

`apply` 会通过 Admin JSON API 写入 Principal 和 access policy。Role 与 RoleBinding 只用于编译，不会作为独立对象保存在 Gateway store 中。

## 预览差异

```bash
cargo run --bin agctl -- diff \
  -f example.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$TOKEN" \
  --prune
```

`--prune` 用于识别同一 RoleBinding 旧版本遗留的 agctl-managed policy。

## 创建 API Key

声明式创建 secret 时需要显式传 `--create-secrets`：

```bash
cargo run --bin agctl -- apply \
  -f example.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$TOKEN" \
  --create-secrets
```

明文 token 只在命令输出中出现一次。生产环境应立即写入 secret manager，不要提交到 Git。

也可以用命令直接创建：

```bash
cargo run --bin agctl -- create api-key svc-order-api \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$TOKEN" \
  --out svc-order-api.gateway.yaml
```

## 本地授权检查

```bash
cargo run --bin agctl -- auth can-i \
  -f example.yaml \
  --as svc-order-api \
  --verb select \
  --resource table \
  --name orders \
  --source mysql-main
```

这个检查只基于 YAML manifest 编译结果，适合在提交前验证授权意图。
