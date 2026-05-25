# agctl YAML Syntax

`agctl` uses Kubernetes-style multi-document YAML to manage Gateway RBAC. Gateway instances are deployment-local: project and environment are not RBAC dimensions. Use separate Gateway deployments for separate projects or environments.

Supported kinds:

| Kind | Purpose |
| --- | --- |
| `Principal` | Calling identity |
| `Role` | Permission rules |
| `RoleBinding` | Binding from a role to a principal |
| `ApiKey` | Declarative API key creation request |

All manifests use:

```yaml
apiVersion: gateway.zenithinc.dev/v1
```

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

Fields:

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `spec.type` | yes | none | `service_account`, `user`, or `legacy_admin` |
| `spec.displayName` | no | empty | Human-readable name |
| `spec.status` | no | `active` | `active` or `disabled` |
| `spec.metadata` | no | `{}` | Object metadata |

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
      resourceNames: ["orders"]
      effect: allow
```

`spec.scope.source` is optional. If omitted, the generated policies match any source. Each rule currently requires exactly one tool, verb, and resource. `resourceNames` may contain `*` wildcards.

Supported resources:

| Resource | Typical Tool |
| --- | --- |
| `table` | `data.query_table` |
| `redis_key` | `redis.query_key` |
| `kubernetes` | Kubernetes tools |
| `app_logs` | `logs.query_app_logs` |
| `audit_events` | `audit.query_approval_events` |

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

`principal` and `role` must reference objects in the same YAML file.

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

`agctl apply` only creates secrets when `--create-secrets` is passed. The plaintext token is printed once.

## Policy Mapping

`Role` and `RoleBinding` are compiled into `accessPolicies`:

| YAML field | Store field |
| --- | --- |
| `RoleBinding.spec.principal` | `principalId` |
| `Role.rule.effect` | `effect` |
| `Role.scope.source` | `sourceName` |
| `Role.rule.tools[0]` | `toolName` |
| `Role.rule.verbs[0]` | `actionName` |
| `Role.rule.resources[0]` | `resourceType` |
| `Role.rule.resourceNames[]` | `resourcePattern` |

## Local Checks

```bash
/opt/action-gateway/bin/agctl auth can-i \
  -f order-api-gateway.yaml \
  --as svc-order-api \
  --verb select \
  --resource table \
  --name orders \
  --source mysql-main
```

## Create Commands

```bash
/opt/action-gateway/bin/agctl create principal svc-order-api \
  --type service_account \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$TOKEN"

/opt/action-gateway/bin/agctl create api-key svc-order-api \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$TOKEN" \
  --out svc-order-api.gateway.yaml
```
