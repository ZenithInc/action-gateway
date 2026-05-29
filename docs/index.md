# Action Gateway

Action Gateway 把 MySQL、Redis、Kubernetes、SLS 日志和审计查询能力暴露为 MCP tools，并用 API Key、policy、source、allowlist 和审计记录控制每一次调用。

如果你想把内部只读排障能力开放给 Agent，但又不想让 Agent 直接拿数据库账号、Redis 账号或 kubeconfig，Action Gateway 适合放在 Agent 和内部系统之间。

## 能力

- 让 Agent 查询 allowlist 内的 MySQL 表，并在查询前经过 `EXPLAIN` 门禁。
- 让 Agent 只读查看 allowlist 内的 Redis key。
- 让 Agent 查询 Kubernetes 资源、Pod 日志和 rollout 状态。
- 让 Agent 查询阿里云 SLS Logstore 和 Gateway 审计摘要。
- 通过 principal、role、role binding、API Key、source 和 allowlist 控制访问范围。

## 推荐路径

面向使用者的主路径是下载 Release 产物并接入自己的环境：

1. [快速开始](/guide/getting-started)：从 GitHub Release 下载发布产物，配置真实 source、allowlist 和 API Key，然后启动 Gateway。
2. [配置 Source 和 Allowlist](/guide/configure-sources)：把 MySQL、Redis、SLS 和 Kubernetes 接入 Gateway。
3. [部署建议](/guide/deployment)：在开发、测试或生产环境部署 Gateway。
4. [接入 MCP Client](/guide/mcp-client)：把 Gateway 配置到 Codex 或其他 MCP Client。

仓库内的 demo stack 只用于项目开发者验证示例流程，不是生产或日常接入的推荐入口。
