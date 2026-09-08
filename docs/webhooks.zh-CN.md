# 签名 Webhook

Webhooks 模块把投递存储在 PostgreSQL Outbox 中，并由后台 worker 发送。当 `RequestContext::publish_webhook` 与请求事务一起调用时，发布是事务化的，因此回滚的业务写入不会泄漏出站事件。

```yaml
modules:
  webhooks:
    enabled: true
    poll_interval_ms: 500
    connect_timeout_ms: 3000
    read_timeout_ms: 10000
    request_timeout_ms: 15000
    endpoints:
      operations:
        url: https://hooks.example.com/appstruct
        secret_env: APPSTRUCT_WEBHOOK_OPERATIONS_SECRET
        events: [project.created, project.archived]
        max_attempts: 4
        backoff_seconds: 3
```

超时值必须在 100 到 25000 毫秒之间。总请求超时必须低于固定的 30 秒投递租约。worker 不消费也不保留响应体；只存储 HTTP 状态，因此无界的下游响应体不会占用应用内存。非成功状态和传输失败使用有上限的指数重试，最终变为 `dead`。

每个请求包含：

- `X-AppStruct-Delivery`：稳定的投递 UUID
- `X-AppStruct-Event`：事件名
- `X-AppStruct-Timestamp`：Unix 时间戳
- `X-AppStruct-Signature`：`v1=` 加小写 HMAC-SHA256

签名字节是 `<timestamp>.<raw-body>`。接收方应使用原始请求体、以恒定时间比较签名、拒绝过期时间戳，并按投递 UUID 去重。

管理员可以在 `/admin/webhooks` 过滤最近投递、原地重试死信投递，或把成功/死信投递重放为新行。Admin API 有意不暴露载荷和签名密钥。多个 worker 可以安全地针对同一数据库运行；领取使用带租约的行锁。
