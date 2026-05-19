# 部署与运维

本页给出生产部署建议。Action Gateway 本身不依赖控制面数据库，运行时状态由 `GATEWAY_STORE_FILE` 指向的 JSON store 保存。

## 部署边界

建议一个 Gateway 实例只服务一个项目和一个环境。例如：

```text
orders-prod      -> 一个 Gateway
orders-staging   -> 另一个 Gateway
platform-prod    -> 另一个 Gateway
```

这样 source、allowlist、policy 和 audit 的边界更容易理解，也更容易回滚。

## 必要组件

| 组件 | 说明 |
| --- | --- |
| Action Gateway binary/container | 提供 `/mcp` 和 `/admin` |
| JSON store 文件 | 保存 source、allowlist、principal、API key hash、policy 和 audit events |
| Redis | 默认 Redis client；也可作为应用日志索引存储 |
| `kubectl` | 如果启用 Kubernetes 工具，Gateway 运行环境必须可执行 `kubectl` |
| Secret manager | 保存 API Key、数据库连接串、Redis URL、kubeconfig |

## 环境变量

| 变量 | 生产建议 | 说明 |
| --- | --- | --- |
| `RPC_BIND_ADDR` | `0.0.0.0:8080` 或内网地址 | Gateway 监听地址 |
| `GATEWAY_STORE_FILE` | 持久化路径 | JSON store 文件 |
| `REDIS_URL` | 内网 Redis URL | 默认 Redis client |
| `RPC_TOKEN` | 不设置 | legacy token，只建议本地 demo 使用 |
| `ACTION_GATEWAY_MCP_TOKEN` | 不设置 | legacy token fallback，只建议本地 Codex demo 使用 |
| `GATEWAY_ALLOW_LEGACY_RPC_TOKEN` | `false` 或不设置 | 非 loopback 是否接受 legacy token |
| `KUBERNETES_ENABLE_RAW_KUBECTL` | `false` | 是否暴露 raw kubectl 诊断工具 |

生产调用方应使用 Gateway API Key：

```text
Authorization: Bearer agk_<key_id>_<secret>
```

## 启动 binary

```bash
cd action-gateway
GATEWAY_STORE_FILE=/var/lib/action-gateway/gateway-store.json \
REDIS_URL=redis://redis.internal:6379/ \
RPC_BIND_ADDR=0.0.0.0:8080 \
cargo run --release
```

如果 `GATEWAY_STORE_FILE` 不存在，Gateway 会创建一个空 store。首次生产部署建议先从 `gateway-store.example.json` 复制并裁剪。

## Docker Compose

仓库内 compose 配置适合本地或内网验证：

```bash
cd action-gateway
RPC_TOKEN=change-me \
GATEWAY_ALLOW_LEGACY_RPC_TOKEN=true \
docker compose --profile gateway up -d redis action-gateway
```

生产环境不要照搬 legacy token 配置。应该改成：

- 挂载持久化 store。
- 从 secret manager 注入 source credential。
- 使用 Gateway API Key。
- 关闭 `GATEWAY_ALLOW_LEGACY_RPC_TOKEN`。

## 生产初始化流程

1. 准备 store 文件，配置 source 和 allowlist。
2. 临时使用 loopback legacy token 或受控 admin API Key 完成 bootstrap。
3. 用 `agctl` 创建平台管理员 Principal。
4. 创建带 `scopes.admin=true` 的 admin API Key。
5. 切换自动化系统使用 admin API Key。
6. 删除或停止使用 legacy token。
7. 给真实 Agent 创建最小权限 API Key。

创建 admin key 示例：

```bash
cargo run --bin agctl -- create principal platform-admin \
  --type service_account \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$GATEWAY_ADMIN_TOKEN"

cargo run --bin agctl -- create api-key platform-admin \
  --endpoint http://127.0.0.1:8080 \
  --admin-token "$GATEWAY_ADMIN_TOKEN" \
  --scopes-json '{"admin":true}' \
  --out platform-admin.gateway.yaml
```

## 健康检查

```bash
curl -s http://127.0.0.1:8080/healthz
```

MCP 工具发现：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $GATEWAY_API_KEY" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

## Store 备份和归档

store 是控制面状态源，需要按敏感数据处理：

- 定期备份 `GATEWAY_STORE_FILE`。
- 限制文件权限，只允许 Gateway 进程和受控运维读取。
- 对 `auditEvents` 做定期归档，避免单个 JSON 文件无限增长。
- 变更 source credential 后递增 `credentialVersion`，便于审计。

## Kubernetes 运行要求

如果使用 Kubernetes 工具：

- Gateway 运行环境必须安装 `kubectl`。
- source credential 必须提供 `kubeconfig` 或 `kubeconfigPath`。
- kubeconfig 对应身份应只具备只读权限。
- `kubernetesResourceAllowlist` 只开放必要 namespace/resource/action。
- 默认不要开启 `KUBERNETES_ENABLE_RAW_KUBECTL`。

## 上线检查清单

- 已为每个项目/环境单独部署 Gateway。
- `GATEWAY_STORE_FILE` 已持久化、备份并限制权限。
- 真实 source credential 没有提交到 Git。
- 所有 Agent 都使用独立 Principal 和 API Key。
- legacy token 已关闭或只绑定 loopback。
- allowlist 从最小范围开始，不使用宽泛正则。
- Kubernetes raw kubectl 默认关闭。
- 已验证 `tools/list`、目标 `tools/call` 和审计查询。
