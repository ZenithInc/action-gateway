# 使用 agctl 管理权限

`agctl` 是 Action Gateway 推荐的权限管理方式。你可以把 Principal、Role、RoleBinding 和 ApiKey 写成多文档 YAML，提交到 Git，再通过 Admin JSON API 应用到 Gateway。

`agctl` 管理的是身份和 access policy。source 和 allowlist 的配置见 [配置 Source 和 Allowlist](/guide/configure-sources)。

注意：`agctl` manifest 不替代 allowlist。manifest 授权某个 Principal 可以调用哪些 tool、访问哪些资源；allowlist 则限制这个 Gateway 实例最多允许触达哪些下游表、Redis key 或 Kubernetes 资源。调用必须同时通过 access policy 和 allowlist。

## 准备 admin token

首次 bootstrap 可以使用 Gateway 启动时配置的 `RPC_TOKEN`：

```bash
export GATEWAY_ADMIN_TOKEN="$RPC_TOKEN"
```

生产环境建议使用带 admin scope 的 API Key，并且只发给受控 CI/CD 或平台自动化。

## 编写权限 manifest

下面示例创建一个服务账号，并允许它查询 `mysql-main` source 中的 `orders` 表：

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
    service: order-api
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

你可以把上面的 manifest 保存为自己的配置文件，例如：

```text
order-api-gateway.yaml
```

## 预览变更

先运行 diff：

```bash
/opt/action-gateway/bin/agctl diff \
  -f order-api-gateway.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$GATEWAY_ADMIN_TOKEN" \
  --prune
```

`--prune` 会识别同一 RoleBinding 旧版本遗留的 agctl-managed policy。

## 应用权限

```bash
/opt/action-gateway/bin/agctl apply \
  -f order-api-gateway.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$GATEWAY_ADMIN_TOKEN" \
  --prune
```

`apply` 会写入 Principal 和 access policy。Role 与 RoleBinding 只用于编译，不会作为独立对象保存在 Gateway store 中。

## 创建 API Key

默认 `apply` 不会创建 secret。需要创建 API Key 时，显式传 `--create-secrets`：

```bash
/opt/action-gateway/bin/agctl apply \
  -f order-api-gateway.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$GATEWAY_ADMIN_TOKEN" \
  --create-secrets
```

命令会输出一次明文 token：

```text
token: agk_<key_id>_<secret>
```

立即把它写入 secret manager 或本地环境变量：

```bash
export GATEWAY_API_KEY='agk_<key_id>_<secret>'
```

也可以直接创建 key，并输出一份 GatewayConfig YAML：

```bash
/opt/action-gateway/bin/agctl create api-key svc-order-api \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$GATEWAY_ADMIN_TOKEN" \
  --out svc-order-api.gateway.yaml
```

`*.gateway.yaml` 包含明文 token，默认已被 `.gitignore` 忽略。

## 本地授权检查

提交权限 YAML 前，可以先在本地检查：

```bash
/opt/action-gateway/bin/agctl auth can-i \
  -f order-api-gateway.yaml \
  --as svc-order-api \
  --verb select \
  --resource table \
  --name orders \
  --source mysql-main
```

返回 `yes` 表示 YAML 编译出的 policy 会允许这次请求；返回 `no` 表示没有匹配的 allow policy 或被 deny policy 命中。

## 常见权限模板

允许读取 Redis key：

```yaml
rules:
  - tools: ["redis.query_key"]
    verbs: ["get"]
    resources: ["redis_key"]
    resourceNames: ["orders:*"]
    effect: allow
```

允许查询某个应用的日志：

```yaml
rules:
  - tools: ["logs.query_app_logs"]
    verbs: ["query"]
    resources: ["app_logs"]
    resourceNames: ["billing-api"]
    effect: allow
```

允许查看 namespace 内所有 Pod：

```yaml
rules:
  - tools: ["kubernetes.list_resources"]
    verbs: ["list"]
    resources: ["kubernetes"]
    resourceNames: ["default/pods/*"]
    effect: allow
  - tools: ["kubernetes.query_pod_logs"]
    verbs: ["logs"]
    resources: ["kubernetes"]
    resourceNames: ["default/pods/*"]
    effect: allow
```

允许查看 Deployment rollout：

```yaml
rules:
  - tools: ["kubernetes.rollout_status"]
    verbs: ["rollout_status"]
    resources: ["kubernetes"]
    resourceNames: ["default/deployments/api"]
    effect: allow
```

如果还需要 rollout history，把 `verbs` 改成 `["rollout_history"]` 或再加一条规则。

## 删除权限

删除某个 YAML 管理的 access policy：

```bash
/opt/action-gateway/bin/agctl delete \
  -f order-api-gateway.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$GATEWAY_ADMIN_TOKEN"
```

同时禁用 manifest 中的 Principal：

```bash
/opt/action-gateway/bin/agctl delete \
  -f order-api-gateway.yaml \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$GATEWAY_ADMIN_TOKEN" \
  --disable-principals
```
