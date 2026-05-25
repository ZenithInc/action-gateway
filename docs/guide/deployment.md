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
- `REDIS_URL` 作为默认 Redis client，也可作为应用日志查询的回退 Redis。

## Secret 和配置

Gateway store 可能包含数据库连接串、Redis URL、kubeconfig、API key hash 和审计摘要。生产环境应按 secret 处理：

- 不要把真实 store 提交到 Git。
- 用 Secret manager、Kubernetes Secret、加密磁盘或受限文件权限保存。
- 给下游 MySQL、Redis 使用只读账号。
- 轮换 source credential 时递增 `credentialVersion`，方便审计定位。

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
