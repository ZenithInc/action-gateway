# Action Gateway

Action Gateway is a controlled MCP gateway that exposes MySQL, Redis, Kubernetes, Alibaba Cloud SLS log, and audit-query capabilities through policy-driven tools.

It is designed to sit between agents and internal systems. Agents receive a Gateway API key; they do not receive database credentials, Redis credentials, or kubeconfig files.

## Capabilities

- **Controlled tools**: read-focused tools for MySQL, Redis, Kubernetes, SLS logs, and audit events.
- **Source isolation**: each MySQL, Redis, SLS, or Kubernetes target is configured as a separate source.
- **Allowlist gates**: MySQL tables, Redis keys, Kubernetes namespaces/resources/actions all require explicit allowlists.
- **Identity and authorization**: principals, roles, role bindings, API keys, and access policies scope each caller.
- **Audit summaries**: tool calls are recorded without storing full business rows, raw log bodies, or Redis values.

## How Users Should Start

The recommended path for users is:

1. Download the release package for your platform from GitHub Releases, or use the matching container image.
2. Prepare a Gateway store and manage it as a secret.
3. Configure real MySQL, Redis, SLS, or Kubernetes sources in the store.
4. Configure `tableAllowlist`, `redisKeyAllowlist`, or `kubernetesResourceAllowlist`.
5. Start `action-gateway`.
6. Use `agctl` to create principals, role bindings, and API keys for callers.
7. Configure Codex or another MCP client with the Gateway endpoint and API key.

See [Getting Started](docs/guide/getting-started.md) for the full flow.

The demo stack in this repository is for project contributors who need local sample data. You do not need to clone the whole repository or run fake-order-service to connect Action Gateway to your own development, staging, or production environment.

## Minimal Configuration

Create `/etc/action-gateway/gateway-store.json`:

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
    },
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
        "accessKeySecret": "<secret>"
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

Start the gateway:

```bash
export GATEWAY_STORE_FILE=/etc/action-gateway/gateway-store.json
export RPC_BIND_ADDR=0.0.0.0:8080
export RPC_TOKEN='<replace-with-admin-bootstrap-token>'
export REDIS_URL='redis://:password@redis.internal:6379/0'

/opt/action-gateway/bin/action-gateway
```

## Tools

| Tool | Description |
| --- | --- |
| `data.query_table` | Query allowlisted MySQL tables with an `EXPLAIN` gate before execution. |
| `redis.query_key` | Read allowlisted Redis keys with output limits. |
| `kubernetes.list_resources` | List allowlisted Kubernetes resources. |
| `kubernetes.get_resource` | Read an allowlisted Kubernetes resource. |
| `kubernetes.query_pod_logs` | Query allowlisted Pod logs. |
| `kubernetes.rollout_status` | Query Deployment / StatefulSet / DaemonSet rollout status or history. |
| `logs.query_sls_logs` | Query Alibaba Cloud SLS Logstore logs. |
| `audit.query_events` | Query Gateway audit event summaries. |

## Documentation

- [Getting Started](docs/guide/getting-started.md)
- [Configure Sources and Allowlists](docs/guide/configure-sources.md)
- [Deployment](docs/guide/deployment.md)
- [MCP Client Setup](docs/guide/mcp-client.md)
- [Store Reference](docs/reference/store.md)
