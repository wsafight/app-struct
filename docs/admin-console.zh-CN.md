# 运维管理控制台

启用 Auth 的项目包含 `/admin` 页面和匹配的 `GET /api/admin/overview` 端点。该端点仅限具有已配置 `admin` 角色的操作者，并报告用户、会话、组织、邀请、排队/死信 Jobs、Mail 投递、Files 和 Audit 事件的实时计数。

已禁用模块的计数器保持为零，并且不会查询未安装的表。该页面链接到运行时生成的详细用户、API token、组织、审计、Jobs、schedules、邮件投递和文件元数据视图。用户视图是只读的，显示账户角色、邮箱验证、创建时间和当前活跃会话；密码和角色变更仍是显式的应用操作。

启用 Jobs 时，`GET /api/admin/jobs` 返回最多 100 条最近 jobs，并支持可选的 `status=queued|running|succeeded|dead` 过滤器。响应包含队列、kind、状态、attempt 预算、时间、租户以及有界的最后错误；载荷故意不暴露。

`POST /api/admin/jobs/{id}/retry` 只接受死信 jobs。它保留原始 ID，重置 attempts，清除租约/错误/完成状态，并立即将 job 入队。`POST /api/admin/jobs/{id}/replay` 接受成功或死信 jobs，并创建包含原始载荷和执行设置的新排队行。重放会清除幂等键，使新 job 不能与原始记录冲突。

所有 Admin 端点都需要 `admin` 角色。通过 cookie 认证的变更还需要生成的 CSRF 头；Bearer tokens 保留其常规 Actor 授权。行锁将重试/重放与竞争的管理操作串行化，排队或运行中的 jobs 不能被重放。

启用 Webhooks 时，`GET /api/admin/webhooks` 返回最多 100 条最近投递，并接受 `status=pending|delivering|succeeded|dead`。它暴露端点/事件名、attempt 状态、响应状态、时间、租户以及有界的最后错误；载荷和端点密钥被省略。

`POST /api/admin/webhooks/{id}/retry` 原地重置死信投递。`POST /api/admin/webhooks/{id}/replay` 把成功或死信投递复制到没有幂等键的新 pending 行。Jobs 使用的同一 admin、CSRF 和行锁要求也适用于投递。

配置了 Jobs schedules 时，`/admin/schedules` 显示它们的 UTC 表达式、状态和运行时间。管理员可以暂停或恢复活动定义，并入队一次立即运行。暂停状态在运行时定义对账后仍然保留；schedule 定义仍由 App Spec 拥有。

启用 Mail 或 Files 时，`/admin/mail` 和 `/admin/files` 提供分页、可搜索的检查。邮件详情渲染文本和转义后的 HTML 源，而不执行它。文件详情只暴露元数据和校验和；它不提供对象下载。

`GET /api/admin/users?limit=50` 返回最多 100 个已注册账户。它只暴露用户身份、角色、验证状态、创建时间，以及未撤销、未过期会话的计数；会话令牌和密码哈希永远不会返回。
