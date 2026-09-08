# 报表

Report v1 提供带版本的模板、经 schema 校验的输入快照、持久 Jobs 执行、通过 File 发布 PDF、保留清理以及生成的 Reports 页面。

它需要 Auth、Jobs 和 File。所选队列必须存在，且 File 必须允许 `application/pdf`。

```yaml
modules:
  jobs:
    enabled: true
    queues:
      reports: { max_attempts: 3, backoff_seconds: 5 }
  file:
    enabled: true
    allowed_content_types: [application/pdf]
  report:
    enabled: true
    queue: reports
    max_input_bytes: 262144
    retention_days: 30
    reader_roles: [auditor, admin]
    templates:
      order-summary:
        version: 1
        body: "Order summary: {{ input.order_id }}"
        input_schema: '{"type":"object","required":["order_id"],"properties":{"order_id":{"type":"string","format":"uuid"}},"additionalProperties":false}'
        data_schema_version: 1
```

`max_input_bytes` 默认为 256 KiB，上限为 4 MiB。`retention_days` 默认为 30，上限为 3650。模板名称包含小写 ASCII 字母、数字、`_` 或 `-`。v1 中模板固定为 PDF；它们的 body digest、JSON Schema、数据 schema 版本和渲染器版本会一起注册，因此已发布版本不能静默变更。

在每个 API 和 worker 进程中将 `APPSTRUCT_REPORT_SNAPSHOT_KEY` 设为 base64 编码的 32 字节 AES-256-GCM 密钥。输入会经过校验、大小检查、使用每次 run 的 nonce 加密，并与 SHA-256 digest 一起存储。缺失或无效的密钥配置会失败关闭。

## API 与授权

- `GET /api/reports/templates`
- `POST /api/reports/templates/{name}/runs`
- `GET /api/reports/runs`
- `GET /api/reports/runs/{id}`
- `POST /api/reports/runs/{id}/cancel`
- `GET /api/reports/runs/{id}/download`

创建需要非空且最多 200 个字符的 `Idempotency-Key`。其范围包括租户、操作者、模板和模板版本。用相同请求重用它会返回原始 run；用不同数据或选项重用它会返回 `REPORT_IDEMPOTENCY_CONFLICT`。

所有读取都绑定租户。run 对其创建者以及具有已配置 `reader_roles` 角色的操作者可见。只有排队中的 runs 可以被取消。下载会重复 run 授权，要求成功的 run，并读取租户绑定的 File 对象。启用 Audit 时，创建、取消和下载会记录为 `report.create`、`report.cancel` 和 `report.download`。

Runs 经过 `queued`、`rendering`、`publishing` 和 `succeeded`；重试耗尽产生 `failed`，被接受的取消产生 `cancelled`。预留的每日 Job 会移除过期的终态 runs 及其结果文件。

生成的客户端为受支持的 JSON Schema 对象提供类型化模板名称和输入形状。Reports 页面创建 runs、轮询活动 jobs、取消排队工作并下载完成的 PDF。

## V1 渲染器边界

附带的 `capture-v1` 渲染器是确定性的，并且有意保持很小：它把 MiniJinja 文本渲染为最小 PDF。它适合契约测试和简单文本报表，但不是 HTML/CSS 或 Chromium 打印引擎；非 ASCII 字形会被替换。沙箱化的生产浏览器渲染器、远程资源、图片、自定义字体以及租户上传的可执行模板仍不在范围内。
