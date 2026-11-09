# P0–P2 产品能力交付

本轮在 `examples/saas-demo` 上依次实现客户可见的产品能力。共享账户、通知、团队、点数、文件和报表契约由生成器提供；看板、审批和 AI 场景位于应用拥有的 `app/backend` 与 `app/web`，不修改基础 SaaS preset v1。已有项目应重新生成并审阅迁移；不要把新的初始建表 SQL 直接覆盖到现有数据库。

## 页面与能力

| 优先级 | 页面 | 交付能力 |
| --- | --- | --- |
| P0 | `/files`、资源详情附件区 | 上传进度/取消，15 分钟暂存会话，Local 代理与 S3 预签名，大小/MIME 校验，完成重放，记录附件、原子替换、解绑和引用保护 |
| P0 | `/results` | 客户任务/结果列表、进度、取消、错误、保留期、下载以及以新任务重试；不暴露运维 Jobs 管理台 |
| P1 | `/notifications` | 分页、未读数、单条/全部已读、偏好；报表、邀请、所有权、审批和 AI 结果通知 |
| P1 | `/account` | 显示名、报表语言/时区默认值、通知偏好、验证新邮箱、改密、撤销会话/API token、TOTP 与一次性恢复码 |
| P1 | `/team`、`/credits` | 团队改名、转让、移除成员，邀请与接受时检查席位；月度点数、购买余额、流水、托管充值入口与退款扣回 |
| P1 | `/admin/insights` | 同意后的第一方访问及 UTM/referrer 来源归因、注册/激活、完整观察的 D7 留存；不同币种的捕获、退款和净额，非银行结算 |
| P2 | `/planning`、`/approvals` | 同一 Task 的看板/日历与跨资源计数；任务快照 + 自有成功报表 + 另一位成员的审批，拒绝/取消/同意及不可变历史 |
| P2 | `/assistant` | 默认禁用的 Ollama 写作示例；点数预占、幂等、队列、SSE 快照、历史、取消和失败返还 |

账户和通知不要求先创建组织。团队、点数、审批、AI 与报表结果使用当前组织；经营视图要求全局 `admin`，无需组织。通知只返回当前组织与账户级事件，切换组织不会泄露其他组织通知。

## 文件契约

- `POST /api/files/uploads`：`{name, content_type, size}` → `{id, upload_url, direct, expires_at}`。每个账户最多 20 个未过期待完成会话。
- 向 `upload_url` PUT 原始字节；`direct=true` 时禁止发送应用 Cookie、CSRF 或组织请求头。S3 Bucket 需允许站点来源的 PUT CORS；不要将整个 Bucket 设置为公开。
- `POST /api/files/uploads/{id}/complete`：返回文件元数据；重放返回相同文件。GET 会话读取状态，DELETE 取消。取消/过期后不能发布。
- `GET /api/files/records/{resource}/{record_id}`、`PUT/DELETE .../{file_id}` 和 `PUT .../{file_id}/replace`，替换体为 `{replacement_file_id}`。`resource` 是生成的表名；UUID/整数 ID 归一化。每个记录/文件作用域最多 100 个附件。
- 关联读取要求记录 read 权限和策略，写入还要求 update 权限和策略。**关联不是新的保密边界**：组织共享库中的文件仍可由该组织库的读者下载；需要更严格保密时应另建私有文件作用域。
- 删除附着文件返回 409；解除引用或真正删除记录后可回收。软删除仍保留引用。暂存清理失败由已有可信存储对账处理；过期不等于定时自动删除暂存元数据。分片、扫描和声明式文件字段不在本轮范围。

共享接口进入生成的 OpenAPI 和 TypeScript 客户端。原代理 POST 上传接口继续兼容。客户端凭证不会转发到 S3。

## 安全、团队和点数

安全操作需要当前密码，已启用 MFA 时还需未使用过的 TOTP 或恢复码。设置 TOTP 前配置独立的 `APPSTRUCT_AUTH_MFA_KEY`（`openssl rand -base64 32`），稳定保管并备份，不能直接替换而不迁移已加密密钥。启用 MFA 撤销其他会话并保留一个新当前会话用于展示恢复码；改密、确认邮箱、禁用 MFA、撤销登录会话也撤销 API token。恢复码仅显示一次，数据库只存哈希。

当前 MFA 是密码登录的第二因子：MFA 账户的 OAuth 登录拒绝，不会跳过第二因子。OAuth-only 用户须先设置密码才能使用这些密码再认证操作。完整 OAuth MFA 挑战、设备列表、账户删除/数据导出不在本轮范围。偏好控制未来通知，不删除历史；报表创建未显式传入语言/时区时读取账户默认值，不提供运行时全站语言切换。

| 环境变量 | 默认值 / 含义 |
| --- | --- |
| `APPSTRUCT_MONTHLY_FREE_CREDITS` | 20，UTC 自然月，未用部分过期 |
| `APPSTRUCT_MONTHLY_PAID_CREDITS` | 200，仅具有 `credits` 权益的有效套餐 |
| `APPSTRUCT_TEAM_SEAT_LIMIT` / `APPSTRUCT_TEAM_PAID_SEAT_LIMIT` | 0 表示不限；付费上限要求 `seats` 权益 |
| `APPSTRUCT_STRIPE_PRICE_CREDITS` / `APPSTRUCT_CREDIT_PACK_UNITS` | 托管一次性充值价格与每包点数（默认 100），不从客户端接受点数价格 |
| `APPSTRUCT_DAILY_VISIT_EVENT_LIMIT` | 每个 UTC 日期最多 100000 个访问事件，限额后返回 429 |

点数先花月度再花购买余额；预占/返还与业务事务一起提交，幂等键不能换账户、组织或成本，已返还的预占不能复用。失败返还到原桶，不复活过期月额度。升级补足当前月额度，降级不追缴当月已授予额度。充值由签名验证后的已支付事件入账（按 `modules.billing.provider` 对应 Stripe Checkout 会话或 Polar 订单）；退款按已捕获金额比例扣回，重复/乱序不重复入账。已消费点数被退款可形成欠额，补足前禁止继续消费。点数不是通用按量计费、订阅席位计价或发票税务系统；报表终身三次免费额度仍是独立的原有规则。

## 应用自有协作接口

通过 `AppExtensions::builder().routes(router)` 注册；共用 CORS、安全头与观测层，**应用路由自身必须调用 `state.context` / `mutation_context` 并验证组织和角色**。重名路由启动失败，不覆盖生成路由。共享可信 helper（`notify`、`reserve_credits` 等）不是客户端授权接口，调用方负责身份、组织和业务校验。

| 接口 | 输入 / 行为 |
| --- | --- |
| `GET /api/product/approvals?page=1` | 当前组织内本人提交或审阅的申请，每页 25 |
| `POST /api/product/approvals` | UUID `Idempotency-Key`，`{task_id, report_run_id, reviewer_id, note}`；同一任务最多一个 pending |
| `POST /api/product/approvals/{id}/decide` | `{decision: approved\|rejected\|cancelled, note}`；审阅者同意/拒绝，发起人取消；相同决定可重放 |
| `GET .../{id}/history` / `GET .../{id}/report` | 不可变决定历史、授权的支持 PDF；遵循报表保留期，并非永久证据归档 |
| `GET /api/product/assistant/capabilities` | 是否配置可用、模型 Provider 与成本/大小上限 |
| `GET/POST /api/product/assistant` | 本人历史（每页 25）/ `{prompt}` + UUID 幂等键提交 |
| `GET .../{id}/events` / `POST .../{id}/cancel` | 经认证 SSE 完整快照/取消；重连替换快照，非字符串重复拼接 |

审批同意要求任务仍与提交快照一致，并原子标记 done、增加 revision、写审计与通知；修改过的任务必须取消后重提。该业务扩展不是任意跨实体 Workflow 引擎，也不替代应用对更复杂权限/Hook 的实现。页面读取/更新使用同一业务数据及乐观并发契约；本轮不承诺跨页面即时实时推送。

AI 仅在 `APPSTRUCT_AI_ENABLED=true` 且设置 `APPSTRUCT_AI_MODEL` 时启用。`APPSTRUCT_AI_ENDPOINT` 默认是本机 Ollama `/api/generate`，仅服务端配置；不要把模型端口暴露给浏览器。`APPSTRUCT_AI_MAX_TOKENS` 默认 512，最大 2048；输入最多 8000 UTF-8 字节、输出最多 64 KiB、每组织最多两个待完成请求、Provider 请求超时 120 秒。每次预占一点，自动恢复任务每分钟处理失联/超过十分钟的工作并返还。队列 `max_attempts: 1` 避免自动重复外部生成；SSE 周期性复核身份/组织，输出仅作文本显示。输出质量和实际模型服务部署需使用自有 Ollama 另行验收。

## 公开页与生产 PDF

生成并安装 Demo Web 依赖后，在仓库根执行 `pnpm saas:build`。脚本仅为 `/`、`/pricing` 生成匿名静态 HTML、标题/描述和 robots；设置 `APPSTRUCT_PUBLIC_SITE_URL=https://your-domain.example` 时再生成 canonical 与 sitemap。构建浏览器禁止外部网络。它不是服务端渲染，也不保证所有自定义公开页自动预渲染；访问同意默认关闭，静态构建不生成访问事件。

生产顺序为：先在示例目录执行 `appstruct build`，再在仓库根执行 `pnpm saas:build`，最后构建 Compose Web 镜像。镜像直接复制 `generated/web/dist`；后续普通构建会覆盖预渲染 HTML，需要重跑预渲染。Demo Nginx 为 AI SSE 显式关闭缓冲与缓存，服务端定时发送保活。

默认 Capture 仅供开发闭环。生产在 `modules.report` 设置 `renderer: chromium`，重新生成，再组合 `compose.production.yaml` 与 `generated/report-renderer/compose.yaml`，配置稳定快照密钥并使用[渲染器隔离与验收](report-renderer.zh-CN.md)。不要在生产中改用无沙箱 Chromium。真实 Stripe 收款、实际 S3 Bucket 的 CORS/凭证及目标平台生产 PDF 仍需自己的部署凭据验收；本地签名事件、模拟模型和浏览器 API 不等于这些外部验收。

## 前端体验优化

1. **移动端与视觉层级**：统一产品卡片、表单和按钮样式；规划页在手机默认使用日程列表，月网格只在自己的可访问滚动区域内横向滚动，不挤压日期格或撑宽页面；指标卡紧凑显示。
2. **中文与格式**：产品页、营销页、导航、审批、AI 和 MFA 使用共享词典；状态显示可读标签，日期/计数按配置格式化，收入按币种显示精确金额并兼容 Stripe 的 ISK/UGX 表示。用户数据和业务标签不会自动翻译。应用语言仍在 `app/web/setup.ts` 配置，不把“报表语言”偏好当作全站语言切换。
3. **查询与刷新**：报表列表、通知角标与收件箱、点数和团队复用租户隔离查询键；只读请求支持取消。报表/AI 仅在当前页存在活动任务时每两秒轮询，终态停止，窗口重新获得焦点时检查更新；待审审批每十五秒刷新，空闲停止。通知/点数等仍保留各自的定时刷新，不承诺实时推送。
4. **操作反馈**：统一骨架、空态、查询重试和成功提示；下载、通知已读、任务重试/取消等按行禁用并去重，其他行仍可操作；提交失败保留输入。AI 输入按实际 UTF-8 字节数提示并限制提交。
5. **导航**：业务入口、数据记录、团队与账户、高级工具分组；桌面保留侧栏，手机可折叠菜单，选择入口后收起，支持 Escape 和焦点返回；原有权限控制保留。
6. **MFA 引导**：本地生成认证器二维码，提供手动密钥、剪贴板失败提示、恢复码复制/下载；用户确认安全保存前不能关闭恢复码，并对站内离开和刷新/关闭页面提供未保存提示。密钥与恢复码不写入浏览器持久存储。

## 验证

`cargo test -p appstruct-codegen --test product_journey` 包含共享 API 与应用审批/AI 测试。无数据库变量时只编译并跳过数据库用例；真实运行设置独立 `APPSTRUCT_E2E_DATABASE_URL`、Capture mail、临时 File Root、测试 snapshot/MFA key、`APPSTRUCT_TEAM_SEAT_LIMIT=2`，并设置测试 Stripe secret/webhook/Pro price。开启 AI 测试还需 `APPSTRUCT_AI_ENABLED=true`、`APPSTRUCT_AI_MODEL=test-model`、`APPSTRUCT_AI_ENDPOINT=http://127.0.0.1:<空闲端口>/api/generate`；测试自己启动该本地 NDJSON stub，不调用真实模型或 Stripe API。

浏览器用例：启动生成的 Demo Web，运行 `APPSTRUCT_E2E_SAAS_PRODUCT=1 PLAYWRIGHT_BASE_URL=http://127.0.0.1:5173 pnpm exec playwright test tests/e2e/saas-product.spec.ts`。它们验证 UI，不代替真实 API/支付验收。渲染器完整链路脚本会重置 public schema，**只能使用全新独立且名称含 test/e2e 的数据库**。

### 本地验证记录（2026-11-06）

数据库、PDF 与全仓测试记录沿用本日前序 P0–P2 验收。本轮前端优化重新执行 Web 单元测试、桌面/手机浏览器用例、类型检查、生成及应用自有代码 ESLint、生产构建/预渲染、生成树快照、Web 契约、Starter 兼容、Rustfmt 与全仓 Clippy；未重复启动数据库或生产 Provider。

| 验证层 | 结果与范围 |
| --- | --- |
| 全仓回归 | `cargo test --workspace` 通过；需要外部数据库的覆盖以单独的真实 PostgreSQL 运行记录为准 |
| 真实 PostgreSQL 产品旅程 | 共享 API 与应用审批/AI 用例通过，覆盖并发额度、退款、附件、MFA、席位、签名支付与租户隔离；模型使用本地 NDJSON stub |
| 数据库结构往返 | Demo 初始迁移后重新读取 PostgreSQL catalog，无列、约束或索引漂移；模块索引四项测试通过 |
| 浏览器与前端 | 14 个 UI 场景分别在桌面和 390px 手机 Chromium 运行，共 28 项通过；生成 Web 单元测试 76 项通过；包含日历无重叠/无页面横溢、终态停止轮询、通知行级并发/共享缓存、MFA 保存保护与剪贴板、AI UTF-8 输入上限；账户设置及规划区 Axe 检查通过，并人工检查截图；类型检查、生成及应用自有代码 ESLint、生产构建通过 |
| 公开静态 HTML | 官网与定价预渲染通过，禁用 JavaScript 后仍验证标题内容、描述和 canonical |
| 真实 PDF | Chromium 渲染、下载、租约续期、执行中取消与过期 worker 禁止发布通过 |
| 静态质量与生成稳定性 | Rustfmt、Clippy `-D warnings`、Prettier、生成树快照和 Starter 兼容测试通过 |

本地 Docker daemon 未启动，未执行生产 Compose 镜像启动验收；真实 Stripe、S3、模型服务及目标部署平台仍需独立验收。上述记录不将模拟 Provider 或本机 Chromium 等同于生产认证。
