# 外部 HTTP 报表渲染器

AppStruct 主框架不内置无头浏览器依赖。生产环境 PDF 报表生成通过 `renderer: http` 插件契约委托给外部 HTTP 渲染服务（兼容 [Gotenberg](https://gotenberg.dev) 或独立部署的 `appstruct-pdf-renderer` 容器）。

本地开发与测试默认继续使用 `capture` 渲染器。

```yaml
modules:
  report:
    renderer: http
    http_url_env: REPORT_RENDERER_URL
    http_secret_env: REPORT_RENDERER_SECRET
    http_timeout_ms: 30000
```

## 运行时环境变量

| 环境变量 | 是否必填 | 说明 |
| --- | --- | --- |
| `REPORT_RENDERER_URL`（或自定义 `http_url_env`） | 是 | 外部 PDF 渲染服务地址。 |
| `REPORT_RENDERER_SECRET`（或自定义 `http_secret_env`） | 可选 | 用于计算 `x-appstruct-signature: sha256=<hex>`（HMAC-SHA256）请求签名的共享密钥。 |

## 契约与边界

- **请求**：`POST <http_url>`，`Content-Type: application/json`，携带 `{ run_id, template, version, html, paper_size, orientation }` 与可选 HMAC 签名头。
- **响应**：`200 OK` 返回 `application/pdf` 字节流，大小受限于 `50 MiB` 与 `modules.file.max_bytes` 中的较小值。
- **安全**：模板由 MiniJinja 转义渲染；数据库凭证、密钥和快照加密密钥绝不透传给外部渲染器。
- **重试**：网络超时与 `5xx` 错误按 `modules.jobs` 预算重试，`4xx` 错误快速失败不重试。
