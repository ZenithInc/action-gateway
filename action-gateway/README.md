# Action Skills MCP Gateway

Action Gateway 通过 HTTP 暴露 MCP JSON-RPC 入口，把受控能力注册为 MCP tools，供 Agent 远程发现和调用。

Gateway 自身不再使用数据库保存配置。Principal、API key、access policy、source、allowlist 和 audit events 都保存在一个 JSON 文件里，由 `GATEWAY_STORE_FILE` 指定，默认是 `gateway-store.json`。旧的 Admin UI/Web 已移除；`/admin` 只保留给 `agctl` 使用的 JSON API。

`data.query_table` 仍然可以连接下游 MySQL 数据源执行只读查询，这是被 Gateway 管理的业务数据源，不是 Gateway 控制面数据库。下游 source 和 allowlist 也写在文件存储里。

## 能力

- `data.query_table`: 查询下游 MySQL 白名单表，查询前执行 `EXPLAIN` 门禁
- `redis.query_key`: 查询 Redis key，只执行只读命令，key 必须命中文件存储里的 `redisKeyAllowlist`
- `kubernetes.list_resources`: 查询 allowlist namespace/resource 的资源列表
- `kubernetes.get_resource`: 查询单个 allowlist 资源摘要
- `kubernetes.rollout_status`: 查询 Deployment/StatefulSet/DaemonSet rollout status/history
- `kubernetes.query_pod_logs`: 查询 allowlist Pod 日志
- `kubernetes.kubectl_read`: raw kubectl 诊断入口，默认隐藏
- `logs.query_app_logs`: 从 Redis `app_logs:*` key 查询应用日志摘要
- `audit.query_approval_events`: 查询文件存储里的认证、授权和动作审计事件

## 文件存储

启动时通过 `GATEWAY_STORE_FILE` 指定状态文件：

```bash
GATEWAY_STORE_FILE=./gateway-store.json cargo run
```

如果文件不存在，Gateway 会创建一个空 JSON store。本目录提供了 [gateway-store.example.json](gateway-store.example.json) 作为结构参考，包含 `sources`、`tableAllowlist`、`redisKeyAllowlist` 和 `kubernetesResourceAllowlist` 示例。

核心字段：

- `principals`: 调用主体，例如 service account 或 user
- `apiKeys`: API key 记录，只保存 salt/hash，不保存明文 secret
- `accessPolicies`: 编译后的授权策略
- `sources`: 下游 MySQL/Redis/Kubernetes source 配置和 credential
- `tableAllowlist`: `data.query_table` 可访问表、列、脱敏和 EXPLAIN 阈值
- `redisKeyAllowlist`: `redis.query_key` 可访问 key 正则和返回大小限制
- `kubernetesResourceAllowlist`: Kubernetes namespace/resource/action 白名单
- `auditEvents`: Gateway 追加写入的审计事件

## Source 怎么确定

Gateway 不区分项目和部署环境。不同项目或环境应独立部署 Gateway，因此授权匹配只使用 `source_name`、tool、action 和资源名。

例如这个请求会匹配 `source=mysql-main`：

```json
{
  "name": "data.query_table",
  "arguments": {
    "source_name": "mysql-main",
    "table_name": "orders",
    "limit": 10
  }
}
```

如果客户端不传 `source_name`，Gateway 使用 `default` source。

## agctl

推荐用 `agctl` 管理生产权限配置，把 `Principal`、`Role`、`RoleBinding`、`ApiKey` 写成多文档 YAML 并提交到 Git。`agctl apply` 会通过 Admin JSON API 写入 Gateway 的文件存储；Role/RoleBinding 本身不持久化，会编译成 `accessPolicies`。

示例 YAML 见 [example.yaml](example.yaml)，完整语法见 [AGCTL_YAML_SYNTAX.md](AGCTL_YAML_SYNTAX.md)。

常用命令：

```bash
cargo run --bin agctl -- apply -f example.yaml --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN"
cargo run --bin agctl -- diff -f example.yaml --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN" --prune
cargo run --bin agctl -- delete -f example.yaml --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN"

cargo run --bin agctl -- create principal svc-order-api --type service_account --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN"
cargo run --bin agctl -- create user alice --display-name "Alice" --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN"
cargo run --bin agctl -- get principals --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN"
cargo run --bin agctl -- get users --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN"

cargo run --bin agctl -- create api-key svc-order-api --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN" --out svc-order-api.gateway.yaml
cargo run --bin agctl -- auth can-i -f example.yaml --as svc-order-api --verb select --resource table --name orders --source mysql-main
```

`apply --create-secrets` 会创建 YAML 中声明的 `ApiKey`，明文 token 只在命令输出中出现一次。`apply --prune` 会禁用同一 RoleBinding 旧版本遗留的 agctl-managed policies。

## Admin JSON API

没有浏览器 Admin UI。Gateway 只提供给 `agctl` 使用的最小管理接口：

- `GET /admin/principals`
- `POST /admin/principals`
- `POST /admin/api-keys`
- `GET /admin/access-policies`
- `POST /admin/access-policies`
- `POST /admin/sources`

这些接口要求调用方是本地 legacy 管理 token，或 API key 的 `scopes.admin=true`。创建 API key 的接口只在响应里返回一次完整 `apiKey`。

## 本地运行

直接启动 Gateway：

```bash
./start.sh
```

启动 Redis、seed demo Redis 数据，并后台运行 Gateway：

```bash
scripts/start-demo-stack.sh
scripts/smoke-demo-stack.sh
```

Compose 只包含 Redis 和 Gateway：

```bash
RPC_TOKEN=change-me GATEWAY_ALLOW_LEGACY_RPC_TOKEN=true docker compose --profile gateway up -d redis action-gateway
```

服务默认监听：

```text
127.0.0.1:8080
```

## 认证

生产身份入口是 Gateway API key：

```text
Authorization: Bearer agk_<key_id>_<secret>
```

API key 明文只在创建时返回一次；文件存储中保存 `secretSalt` 和 `secretHash`。本地/demo 可用 `RPC_TOKEN` 作为 unrestricted legacy token；如果未设置 `RPC_TOKEN`，Gateway 会读取 `ACTION_GATEWAY_MCP_TOKEN` 作为 fallback，方便直接配合 Codex MCP 配置使用。非 loopback 绑定时，只有显式设置 `GATEWAY_ALLOW_LEGACY_RPC_TOKEN=true` 才接受 legacy token。

## MCP 调用

初始化：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer change-me' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"example-agent","version":"0.1.0"}}}'
```

列出工具：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer change-me' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```

查询 Redis key：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer change-me' \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"redis.query_key","arguments":{"key":"demo:user:1","limit":20}}}'
```

查询应用日志：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer change-me' \
  -d '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"logs.query_app_logs","arguments":{"app_name":"billing-api","environment":"prod","keyword":"12.00","limit":20}}}'
```

健康检查：

```bash
curl -s http://127.0.0.1:8080/healthz
```
