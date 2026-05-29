# 安全边界

Action Gateway 只有在 Agent 不能修改 Gateway 配置、不能读取 Gateway secret、不能重启或替换 Gateway、不能直连下游系统时，才可以作为生产查询的安全边界。

如果 Codex 或其他 Agent 与 Gateway 运行在同一台主机、同一个容器、同一个可写工作区，且 Agent 能修改 store、环境变量、启动脚本或进程参数，那么 Agent 可以启动新的 Gateway 实例，或者改写 allowlist 和 source 配置后查询未授权资源。此时 Gateway 只是一个可被 Agent 改写的本地客户端封装，不能被当作生产访问控制边界。

## 生产部署建议

- 独立部署 Gateway。使用单独 VM、Kubernetes namespace、容器编排工作负载或受控 systemd service 承载 Gateway，Agent 运行环境不能写入 Gateway 二进制、store、启动脚本或环境变量。
- Agent 只持有调用 Gateway 的短期 API Key，不能持有 `RPC_TOKEN`、Gateway store、数据库连接串、Redis URL、kubeconfig 或云厂商凭证。
- 网络层只允许 Agent 访问 Gateway 的 `/mcp` endpoint，禁止 Agent 直连生产 MySQL、Redis、Kubernetes API Server 或日志后端。
- 下游系统继续做最小权限。MySQL 使用只读账号、view 和列权限；Redis 使用 ACL 和窄 key 前缀；Kubernetes 使用只读、窄 namespace/resource 的 service account。
- 防止假 Gateway。生产凭证只挂载到正式部署的 Gateway 工作负载；下游服务可以结合 mTLS、workload identity、固定 egress 来源或 service mesh policy 只信任正式 Gateway。
- Admin API 和 `agctl apply` 只允许 CI/CD、堡垒机或平台管理员访问。不要让 Agent 持有管理 token。
- 审计日志写到 Agent 不可写的位置；生产环境优先使用外部日志、SIEM 或 append-only 存储保留管理变更和工具调用摘要。

## 与本地开发的区别

本地开发可以把 Gateway、store 和 Codex 放在同一台机器上验证流程，但这不提供生产级隔离。只要 Agent 能读取 store 或改写 Gateway 运行参数，它就能绕过 Gateway 声明的资源边界。
