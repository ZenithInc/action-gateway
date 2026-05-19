---
layout: home

hero:
  name: Action Gateway
  text: 面向 Agent 的受控操作网关
  tagline: 把 MySQL、Redis、Kubernetes、应用日志和审计查询能力暴露为 MCP tools，并用 API Key、policy、source、allowlist 和审计记录控制每一次调用。
  actions:
    - theme: brand
      text: 10 分钟跑通
      link: /guide/getting-started
    - theme: alt
      text: 接入 Codex
      link: /guide/mcp-client

features:
  - title: 可直接接入 Agent
    details: 提供 HTTP JSON-RPC MCP endpoint，Agent 通过 tools/list 发现能力，通过 tools/call 调用工具。
  - title: 面向生产的权限边界
    details: 每个调用都会经过 Principal、API Key、access policy、source 和 allowlist 检查。
  - title: 文件化控制面
    details: Gateway 状态保存在 JSON store 中，便于备份、审计和 GitOps 化权限管理。
---

## 这份文档适合谁

如果你想把内部只读排障能力开放给 Agent，但又不想让 Agent 直接拿数据库账号、Redis 账号或 kubeconfig，Action Gateway 适合放在 Agent 和内部系统之间。

你可以用它完成这些工作：

- 让 Agent 查询 allowlist 内的 MySQL 表，并在查询前经过 `EXPLAIN` 门禁。
- 让 Agent 只读查看 allowlist 内的 Redis key。
- 让 Agent 查看 Kubernetes 资源摘要、rollout 状态和 Pod 日志。
- 让 Agent 查询应用日志索引和 Gateway 审计事件。
- 用 `agctl` 把生产权限写成 YAML，并在变更前后做 diff。

## 从零到可用

建议按下面顺序阅读：

1. [快速开始](/guide/getting-started)：本地启动 Gateway，完成一次 MCP 初始化、工具发现和 Redis 查询。
2. [核心概念](/guide/concepts)：理解 Principal、API Key、source、allowlist 和 access policy 的关系。
3. [配置 Source 和 Allowlist](/guide/configure-sources)：把 demo 配置换成真实 MySQL、Redis 或 Kubernetes。
4. [使用 agctl 管理权限](/guide/agctl)：创建调用主体、权限规则和 API Key。
5. [接入 Codex](/guide/mcp-client)：把 Gateway 作为 Codex 的 MCP server 使用。
6. [部署与运维](/guide/deployment)：准备生产环境、持久化 store、关闭 legacy token，并规划备份和审计。

## 当前兼容性状态

Action Gateway 目前只测试过 Codex 作为 MCP client。其他兼容 MCP 的客户端理论上可以通过同一个 HTTP JSON-RPC 接口接入，但暂未验证。
