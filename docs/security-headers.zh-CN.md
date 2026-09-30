# 安全响应头

生成的后端会为所有路由附加一组基础安全响应头。默认即开启，因此即使项目里完全没有提到
`server:`，也会输出 `X-Content-Type-Options`、`X-Frame-Options` 和 `Referrer-Policy`，并在
`APPSTRUCT_ENV=production` 时追加 `Strict-Transport-Security`。

```yaml
server:
  security_headers:
    enabled: true
    hsts: auto
    frame_options: deny
    referrer_policy: strict-origin-when-cross-origin
    content_security_policy: null
    custom:
      x-robots-tag: noindex
```

## 默认值

| 配置项 | 默认值 | 输出的响应头 |
| --- | --- | --- |
| `enabled` | `true` | — |
| `hsts` | `auto` | `Strict-Transport-Security: max-age=31536000; includeSubDomains`，仅生产环境 |
| `frame_options` | `deny` | `X-Frame-Options: DENY` |
| `referrer_policy` | `strict-origin-when-cross-origin` | `Referrer-Policy: strict-origin-when-cross-origin` |
| `content_security_policy` | 未设置 | `Content-Security-Policy`，仅在显式配置时输出 |
| `custom` | 空 | 每项一个响应头 |

只要 `enabled` 为 `true`，`X-Content-Type-Options: nosniff` 始终输出，没有单独的开关。

## 可选项

- `enabled: false` 关闭全部响应头。若在关闭状态下仍配置了 `content_security_policy` 或 `custom`，
  会输出 `AS3108` 警告，因为这些值不会生效。
- `hsts` 接受 `auto`、`off` 或非负的 max-age 秒数。`auto` 仅在 `APPSTRUCT_ENV=production` 时输出；
  显式数字则始终输出。不会生成 `preload`。
- `frame_options` 接受 `deny`、`sameorigin` 或 `off`。
- `referrer_policy` 接受 `no-referrer`、`same-origin`、`strict-origin-when-cross-origin` 或 `off`。
- `content_security_policy` 是字面策略字符串，默认不生成：Web 运行时是单页应用并使用内联样式，
  有用的策略取决于应用自身的资源和 API 来源。
- `custom` 追加任意响应头。名称必须是小写 ASCII 字母、数字和连字符，值必须是可见 ASCII 字符或
  水平制表符。自定义头使用 `if_not_present`，因此不会覆盖内层已经设置的头（CORS、`ETag`）。

## 生效范围

生成的 `apply_security_headers` 辅助函数包裹整个 Axum 路由，因此响应头覆盖资源路由、模块路由
（`/auth`、`/audit`、`/billing`、`/realtime`、`/report`、`/activity`、租户路由）、
`/health/live`、`/health/ready`、`/metrics` 和 `/openapi.json`。

由于该层位于最外层，它在响应路径上最后执行。这正是 `custom` 采用 `if_not_present` 的原因：
用户自定义的 `access-control-allow-origin` 或 `etag` 不会覆盖 CORS 或 revision 层已生成的值。
