# 记录动态

Activity v1 为选定实体添加协作时间线，同时把业务对话与不可变的 Audit 历史分开。它支持评论、可选附件、系统事件、创建者撤回、管理员删除、游标分页以及生成的详情页 UI。

Auth 和 Audit 是必需的。启用附件还需要 File。

```yaml
modules:
  auth:
    enabled: true
    user_entity: User
  audit:
    enabled: true
    reader_roles: [admin]
  file:
    enabled: true
    allowed_content_types: [text/plain, image/png]
  realtime:
    enabled: true
  activity:
    enabled: true
    max_comment_bytes: 4000
    attachments: true
    admin_roles: [admin]
    resources: [Order]
```

`resources` 包含实体名称。生成的 URL 和 TypeScript 资源键是实体的表名，例如 `orders`；任意客户端提供的资源类型会被拒绝。评论大小按服务端裁剪并移除控制字符后的 UTF-8 字节计量。可配置限制默认为 4000 字节，必须介于 1 和 65536 之间。

## API 与授权

对于资源 `orders` 和记录 `42`，生成的 API 是：

- `GET /api/activity/orders/42?cursor=...&limit=20`
- `POST /api/activity/orders/42/comments`
- `POST /api/activity/orders/42/{entry_id}/withdraw`
- `POST /api/activity/orders/42/{entry_id}/moderate`
- 启用附件时为 `GET /api/activity/orders/42/{entry_id}/attachment`

每个操作都会重新应用目标实体的租户范围、声明式读访问和扩展 Policy。缺失或不可读的记录不会通过 Activity 暴露。变更端点需要 cookie 认证和 CSRF 保护；列表和下载也支持 bearer 认证。

评论作者可以撤回自己的活跃评论。具有 `admin_roles` 角色的操作者可以管理活跃评论，并且必须提供原因。两种操作都保留墓碑：正文和附件元数据会被清除，撤回元数据保留，治理操作写入 Audit。附件只接受名称、内容类型和 base64 内容。服务器通过 File 校验它们并生成对象键；客户端不能选择 bucket、路径或对象键。

分页按 `(occurred_at, id)` 降序排列，并返回不透明的 `next_cursor`；限制为 1 到 100。稳定的 Activity 专用错误是 `UNKNOWN_ACTIVITY_RESOURCE`（404）、`INVALID_ACTIVITY_INPUT`（422）和 `ACTIVITY_ALREADY_WITHDRAWN`（409），以及常规的 auth、租户、查询和未找到错误。

## 系统与实时事件

创建、更新、删除、批量/CSV 写入、软删除恢复以及 Workflow 转换会在与业务写入同一数据库事务中添加系统条目。事件名是 `created`、`updated`、`deleted`、`restored` 和 `workflow.<action>`。系统条目存储事件名，不会复制 Audit 的 before/after 快照。

启用 Realtime 时，时间线会订阅当前记录。它会为远程评论治理、CRUD/恢复以及已声明的 Workflow 事件刷新，并对重复的 SSE 事件 ID 去重。没有 Realtime 时，同一 UI 仍然可用，并在本地变更后刷新。

Activity v1 不实现提及、反应、富文本、评论编辑、线程、任意事件种类或持久通知收件箱。条目故意没有指向业务表的多态数据库外键；授权通过解析已声明资源并在每个请求上重新检查其读 Policy 来强制执行。
