---
layout: home

hero:
  name: Action Gateway
  text: 面向 Agent 的受控操作网关
  tagline: 通过 MCP JSON-RPC 暴露数据库、Redis、Kubernetes、日志和审计查询能力，并用文件化配置、API Key、allowlist 和 access policy 控制每一次调用。
  actions:
    - theme: brand
      text: 快速开始
      link: /guide/getting-started
    - theme: alt
      text: MCP Client 接入
      link: /guide/mcp-client

features:
  - title: MCP 工具化
    details: 将受控能力注册为 MCP tools，Agent 可以远程发现并调用。
  - title: 文件化权限
    details: Principal、API Key、access policy、source、allowlist 和审计事件都保存在 JSON store 中。
  - title: 最小暴露面
    details: 数据、Redis 和 Kubernetes 能力默认受 allowlist、只读约束、输出上限和审计保护。
---

## 适用场景

Action Gateway 适合把内部运维和排障能力以标准 MCP 接口提供给 Agent，同时保留清晰的权限边界和审计记录。

典型能力包括：

- 查询 allowlist 内的 MySQL 表，并在查询前执行 `EXPLAIN` 门禁。
- 只读读取 allowlist 内的 Redis key。
- 列出、获取 Kubernetes allowlist 资源，并查询 rollout 状态和 Pod 日志。
- 从 Redis 日志索引查询应用日志摘要。
- 查询认证、授权和动作审计事件。

## 推荐阅读路径

1. 从 [快速开始](/guide/getting-started) 启动本地 Gateway。
2. 阅读 [核心概念](/guide/concepts) 理解 Principal、API Key、source、allowlist 和 policy。
3. 使用 [agctl 教程](/guide/agctl) 管理生产权限。
4. 按 [MCP Client 接入](/guide/mcp-client) 接入 Agent 或自定义客户端。
