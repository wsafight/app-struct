# 实时事件、在线状态与编辑租约

Realtime 模块提供已认证的 Server-Sent Events、基于 PostgreSQL 的在线状态，以及可选的独占编辑租约。每个请求都需要具体的资源作用域。记录作用域还会运行该资源生成的读 Policy；集合作用域运行其 list Policy。

```yaml
modules:
  realtime:
    enabled: true
    heartbeat_seconds: 15
    presence_ttl_seconds: 45
```

使用生成的 `subscribeRealtime({ resource, recordId? })` 辅助函数订阅。事件会按租户、资源、可选记录以及订阅者当前读 Policy 过滤。生成的 CRUD 事件向浏览器只暴露 `{ resource, record_id }`；原始模型会短暂保留在 `_appstruct_realtime_events` 中，以便每个副本在投递前执行行级授权。

每个 API 进程会立即广播本地写入，并轮询 PostgreSQL 事件表以获取其他副本的写入。预期跨副本延迟约为 100 毫秒。超过五分钟的行会定期清理。这是实时更新通道，不是持久事件日志：断开连接或收到 `resync` 事件的客户端必须重新加载其资源查询。

在线状态行带有数据库 TTL，由 SSE 心跳续期，并在干净断开时删除。即使尚未清理，过期行也不会出现在读取结果中。资源级在线状态列表不会泄露记录级会话；必须显式请求已授权的 `recordId` 才能看到它们。

生成的锁辅助函数实现按需独占租约：

- `acquireRealtimeLock(scope, ttlSeconds)`
- `getRealtimeLock(scope)`
- `renewRealtimeLock(scope, token, ttlSeconds)`
- `releaseRealtimeLock(scope, token)`

TTL 为 5 到 300 秒，默认 30。当另一份未过期租约存在时，获取会返回 `409`；过期后，另一 actor 可以获取新 Token。续期和释放需要拥有者 actor、Token、记录 Policy 和 CSRF 校验。CRUD 不会自动要求租约，因此应用可以只在需要独占编辑的工作流中采用锁。
