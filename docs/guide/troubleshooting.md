# 故障排查

本页按症状列出常见问题和检查命令。

## Gateway 没启动

检查健康接口：

```bash
curl -s http://127.0.0.1:8080/healthz
```

如果使用 systemd，查看服务状态和日志：

```bash
systemctl status action-gateway
journalctl -u action-gateway -n 100
```

如果使用 Kubernetes，查看 Pod 和日志：

```bash
kubectl get pods -l app=action-gateway
kubectl logs deploy/action-gateway --tail=100
```

## 端口被占用

调整 `RPC_BIND_ADDR` 中的端口，例如 `0.0.0.0:8081`，然后重启 Gateway。后续 curl 和 MCP Client 配置都要使用同一个端口。

## `401 unauthorized`

检查：

- 请求是否带了 `Authorization: Bearer ...`。
- 生产 API Key 是否是 `agk_<key_id>_<secret>` 格式。
- Principal 或 API Key 是否已被禁用。
- API Key 是否过期。

## `tools/list` 能成功，但 `tools/call` 未授权

这通常是 access policy 不匹配。检查 YAML：

```bash
/opt/action-gateway/bin/agctl auth can-i \
  -f order-api-gateway.yaml \
  --as svc-order-api \
  --verb select \
  --resource table \
  --name orders \
  --source mysql-main
```

重点检查：

- `principal` 是否和 API Key 绑定的 principal 一致。
- `source` 是否和工具参数里的 `source_name` 一致。
- tool、verb、resource 是否匹配。
- `resourceNames` 是否匹配真实资源名。

## 返回 `not allowlisted`

这表示 access policy 可能已经通过，但 allowlist 没放行。

检查：

- MySQL：`tableAllowlist` 是否包含对应 `sourceName` 和 `tableName`。
- Redis：`redisKeyAllowlist.keyPattern` 是否完整匹配 key。
- Kubernetes：`kubernetesResourceAllowlist` 是否包含 namespace、resource 和 action。

手工修改 store 后需要重启 Gateway。

## MySQL 查询失败

常见原因：

- `sources` 中没有 `sourceType: "mysql"` 的对应 source。
- credential 中缺少 `url`、`connectionUrl` 或 `databaseUrl`。
- 数据库账号没有只读权限。
- 查询列或 filter 字段没有在 `tableAllowlist.columns` 中。
- `EXPLAIN` 预估扫描行数超过 `maxEstimatedRows`。

## Redis 查询失败

常见原因：

- Redis URL 不可达。
- key 没有命中 `redisKeyAllowlist.keyPattern`。
- 返回值超过 `maxValueBytes`。
- 集合成员数超过或请求 limit 超过 `maxMembers`。

## Kubernetes 查询失败

常见原因：

- Gateway 运行环境没有安装 `kubectl`。
- source credential 缺少 `kubeconfig` 或 `kubeconfigPath`。
- kubeconfig 对应身份没有读取目标 namespace/resource 的权限。
- `kubernetesResourceAllowlist.actions` 没有包含当前工具对应 action。
- `KUBERNETES_ENABLE_RAW_KUBECTL` 未开启，但调用了 `kubernetes.kubectl_read`。

## Codex 看不到工具

先绕过 Codex 验证 Gateway：

```bash
curl -s http://127.0.0.1:8080/mcp \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ACTION_GATEWAY_API_KEY" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

如果 curl 成功：

- 检查 `.codex/config.toml` 是否在启动 Codex 的项目目录下。
- 检查 `url` 端口是否正确。
- 检查 `bearer_token_env_var` 指向的环境变量是否在 Codex 进程启动前设置。

如果 Codex 启动时报类似 `Unexpected content type: Some("missing-content-type; body: ")`，但直接 curl Gateway 能成功，重点检查 Codex 所在机器是否设置了代理环境变量，例如 `HTTP_PROXY`、`HTTPS_PROXY` 或 `ALL_PROXY`。代理可能会截走发往 `127.0.0.1`、`localhost` 或内网 Gateway 域名的 MCP 请求，导致握手响应不是 Gateway 返回的 JSON。

先用不走代理的 curl 验证：

```bash
curl --noproxy "*" -i http://127.0.0.1:8080/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Authorization: Bearer $ACTION_GATEWAY_API_KEY" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0.1.0"}}}'
```

如果这样成功，在启动 Codex 前设置 `NO_PROXY` 和 `no_proxy`，至少包含 Gateway host：

```bash
export NO_PROXY="127.0.0.1,localhost,::1,gateway.example.com"
export no_proxy="$NO_PROXY"
```

设置后必须重新启动 Codex。

如果 curl 失败，先按本页前面的认证、端口和 Gateway 状态排查。
