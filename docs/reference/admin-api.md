# Admin JSON API

Gateway 不提供浏览器 Admin UI。`/admin` 只保留给 `agctl` 和自动化系统使用。

## 认证

Admin API 要求调用方满足以下条件之一：

- 使用本地 legacy 管理 token。
- 使用 API Key，且 key 的 `scopes.admin=true`。

生产环境建议只把 admin scope 授予受控自动化系统，并保留审计。

## Endpoints

| Method | Path | 用途 |
| --- | --- | --- |
| `GET` | `/admin/principals` | 列出 Principal |
| `POST` | `/admin/principals` | 创建或更新 Principal |
| `POST` | `/admin/api-keys` | 创建 API Key |
| `GET` | `/admin/access-policies` | 列出 access policy |
| `POST` | `/admin/access-policies` | 创建或更新 access policy |
| `POST` | `/admin/sources` | 创建或更新 source |

创建 API Key 的接口只在响应中返回一次完整 `apiKey`。客户端必须立即保存明文 token。

## 推荐用法

除非正在开发 `agctl` 或做自动化集成，否则不要直接手写 Admin API 请求。常规生产变更应通过：

```bash
cargo run --bin agctl -- diff -f example.yaml --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN"
cargo run --bin agctl -- apply -f example.yaml --endpoint http://127.0.0.1:8080 --admin-token "$TOKEN"
```
