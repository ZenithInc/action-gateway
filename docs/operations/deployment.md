# 部署建议

本页面向把 Action Gateway 部署到开发、测试或生产环境的使用者。推荐从 GitHub Release 获取二进制或容器镜像，再用 Secret、ConfigMap 或挂载文件注入 Gateway store 和运行时环境变量。

仓库里的 demo stack 只用于本地开发验证，不是部署入口。

## 部署形态

| 形态 | 适用场景 |
| --- | --- |
| systemd / 进程管理器 | 单机开发、测试环境或小规模内部服务 |
| Kubernetes Deployment | 生产环境、需要统一发布和 Secret 管理 |
| 容器编排平台 | 已有镜像发布链路的团队 |

无论哪种形态，核心配置都相同：

- `action-gateway` 服务端进程。
- `GATEWAY_STORE_FILE` 指向 Gateway store。
- `RPC_BIND_ADDR` 指定监听地址。
- `RPC_TOKEN` 用于首次管理和 bootstrap。
- `REDIS_URL` 作为 `redis.query_key` 未配置 Redis source 时的默认 Redis client。

## Secret 和配置

Gateway store 可能包含数据库连接串、Redis URL、kubeconfig、API key hash 和审计摘要。生产环境应按 secret 处理：

- 不要把真实 store 提交到 Git。
- 用 Secret manager、Kubernetes Secret、加密磁盘或受限文件权限保存。
- 给下游 MySQL、Redis 使用只读账号。
- 轮换 source credential 时递增 `credentialVersion`，方便审计定位。

## Agent 与 Gateway 的安全边界

> **警告**
>
> 生产环境不要把可写 Gateway 放在 Agent 同一信任域内。Action Gateway 只有在 Agent 不能修改 Gateway 配置、不能读取 Gateway secret、不能重启或替换 Gateway、不能直连下游系统时，才可以作为生产查询的安全边界。

如果 Codex 或其他 Agent 与 Gateway 运行在同一台主机、同一个容器、同一个可写工作区，且 Agent 能修改 store、环境变量、启动脚本或进程参数，那么 Agent 可以启动新的 Gateway 实例，或者改写 allowlist 和 source 配置后查询未授权资源。此时 Gateway 只是一个可被 Agent 改写的本地客户端封装，不能被当作生产访问控制边界。

生产环境建议按下面方式部署：

- 独立部署 Gateway。使用单独 VM、Kubernetes namespace、容器编排工作负载或受控 systemd service 承载 Gateway，Agent 运行环境不能写入 Gateway 二进制、store、启动脚本或环境变量。
- Agent 只持有调用 Gateway 的短期 API Key，不能持有 `RPC_TOKEN`、Gateway store、数据库连接串、Redis URL、kubeconfig 或云厂商凭证。
- 网络层只允许 Agent 访问 Gateway 的 `/mcp` endpoint，禁止 Agent 直连生产 MySQL、Redis、Kubernetes API Server 或日志后端。
- 下游系统继续做最小权限。MySQL 使用只读账号、view 和列权限；Redis 使用 ACL 和窄 key 前缀；Kubernetes 使用只读、窄 namespace/resource 的 service account。
- 防止假 Gateway。生产凭证只挂载到正式部署的 Gateway 工作负载；下游服务可以结合 mTLS、workload identity、固定 egress 来源或 service mesh policy 只信任正式 Gateway。
- Admin API 和 `agctl apply` 只允许 CI/CD、堡垒机或平台管理员访问。不要让 Agent 持有管理 token。
- 审计日志写到 Agent 不可写的位置；生产环境优先使用外部日志、SIEM 或 append-only 存储保留管理变更和工具调用摘要。

更完整的隔离原则见 [安全边界](/action-gateway/operations/security-boundary/)。

## 最小运行命令

```bash
export GATEWAY_STORE_FILE=/etc/action-gateway/gateway-store.json
export RPC_BIND_ADDR=0.0.0.0:8080
export RPC_TOKEN='<replace-with-admin-bootstrap-token>'
export REDIS_URL='redis://:password@redis.internal:6379/0'

/opt/action-gateway/bin/action-gateway
```

## systemd 示例

```ini
[Unit]
Description=Action Gateway
After=network-online.target
Wants=network-online.target

[Service]
User=action-gateway
Group=action-gateway
Environment=GATEWAY_STORE_FILE=/etc/action-gateway/gateway-store.json
Environment=RPC_BIND_ADDR=0.0.0.0:8080
EnvironmentFile=/etc/action-gateway/action-gateway.env
ExecStart=/opt/action-gateway/bin/action-gateway
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

`/etc/action-gateway/action-gateway.env` 中保存敏感环境变量：

```bash
RPC_TOKEN=<replace-with-admin-bootstrap-token>
REDIS_URL=redis://:password@redis.internal:6379/0
```

## Kubernetes 示例

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: action-gateway
spec:
  replicas: 1
  selector:
    matchLabels:
      app: action-gateway
  template:
    metadata:
      labels:
        app: action-gateway
    spec:
      containers:
        - name: action-gateway
          image: ghcr.io/<org>/action-gateway:<version>
          ports:
            - containerPort: 8080
          env:
            - name: GATEWAY_STORE_FILE
              value: /etc/action-gateway/gateway-store.json
            - name: RPC_BIND_ADDR
              value: 0.0.0.0:8080
            - name: RPC_TOKEN
              valueFrom:
                secretKeyRef:
                  name: action-gateway-secret
                  key: rpc-token
            - name: REDIS_URL
              valueFrom:
                secretKeyRef:
                  name: action-gateway-secret
                  key: redis-url
          volumeMounts:
            - name: gateway-store
              mountPath: /etc/action-gateway
              readOnly: true
      volumes:
        - name: gateway-store
          secret:
            secretName: action-gateway-store
```

如果需要通过 Admin API 或 `agctl apply` 在运行时写入 store，挂载位置必须可写，或者使用持久卷承载 store 文件。只读 Secret 更适合把 store 作为声明式配置发布。

## 网络与权限

- Gateway 到 MySQL / Redis / Kubernetes API Server 需要网络可达。
- MCP Client 只需要访问 Gateway 的 `/mcp` endpoint。
- Admin API 应限制在内网、堡垒机或 CI/CD 网络内。
- 下游账号应只授予排障所需的最小权限。
- Redis key 和 MySQL table 仍必须通过 allowlist 才能访问。

## 上线前检查

- `tools/list` 只返回目标调用方需要的工具。
- MySQL 查询命中预期 `tableAllowlist`，敏感字段有 `maskRules`。
- Redis key 正则足够窄，未使用 `.*` 这类宽泛规则。
- Gateway store 和环境变量已纳入备份与轮换流程。
- 审计事件有保留、归档或清理策略。
