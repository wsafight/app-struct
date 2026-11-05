# Wasp / Open SaaS 产品能力复核

> 日期：2026-11-05
> 性质：开发前的源码与官方文档对标快照；第 3 节状态描述对标时基线，后续首轮实现见 [SaaS 产品闭环](saas-product-journey.zh-CN.md) 和 [路线图交付状态](next-product-roadmap.zh-CN.md#6-产品能力首轮交付2026-11-05)
> 本项目基线：`df1e9c2` 加当前工作区改动

## 1. 结论

AppStruct 已经覆盖大量后台基础能力，下一步需要增加客户可以直接使用的应用能力。
这次对标支持把 SaaS 入口推进到“访客了解产品、注册、使用业务功能、付费、继续使用、经营者看结果”的完整旅程。
租户索引、存储对账和发布检查继续作为质量基础，不能代替上述产品能力的交付。

Wasp 提供自由构建应用的框架能力；Open SaaS 在其上提供获客、账户、支付、文件和经营页面。
AppStruct 的现有数据库接入、租户隔离、声明式权限、审计、数据管理和受控迁移继续构成自身产品价值。
两个入口应共享内核：已有 PostgreSQL 到后台，以及新建可运营的 SaaS 应用。

## 2. 拉取与证据范围

本次从官方仓库克隆默认分支，避免改动已有且不干净的 `references/wasp`：

| 项目 | 提交 | 提交日期 | 本地目录 |
| --- | --- | --- | --- |
| Wasp | `848143a0039abeb27b1732c7eb39510ca73c7949` | 2026-11-05 | `target/product-reference-review/wasp` |
| Open SaaS | `c2c6285bdb14b040584505372ad9bb062a0bca11` | 2026-10-28 | `target/product-reference-review/open-saas` |

Wasp 使用稀疏检出，读取文档、示例与 starter；Open SaaS 读取实际模板、业务实现和文档。
这些是默认分支源码快照，不等同于最新稳定发布，也没有在本次执行参考项目的部署或支付验收。
官方文档通过 Open SaaS 的 `llms.txt` 与对应原始 Markdown 核对；文档中的旧包名和旧路径不覆盖上述源码证据。

## 3. 按用户任务对照

| 用户任务 | Wasp / Open SaaS 中的证据 | AppStruct 当前状态与差距 |
| --- | --- | --- |
| 访客了解产品并注册 | Open SaaS 有 Landing、FAQ、介绍、定价与独立账户页面；Wasp 支持公开路由、布局和静态预渲染。[S1][S2] | 已有公开认证页；启用 Auth 后，自定义页面进入认证及租户后台布局。缺少公开营销页、个人账户页与租户工作台各自的路由作用域。框架自己的 `site/` 不能代替生成项目的官网。 |
| 购买并实际获得业务功能 | Open SaaS 有订阅、一次性点数包、账户余额，以及业务动作中的订阅检查、原子扣点和失败返还。[S3] | Stripe 订阅、试用、门户、Webhook 和权益列表已经存在；缺少可复用的后端权益校验、配额/点数机制及对应升级提示。当前不能把返回权益字符串算作业务功能已受套餐控制。 |
| 上传和管理自己的文件 | Open SaaS 有文件页、进度、S3 预签名上传、元数据登记与下载。[S4] | 已有 Local/S3 Provider、生命周期、管理员元数据页，以及 Activity 附件和报告文件。缺少通用的终端用户上传组件、文件列表、业务字段绑定和直传完成协议。 |
| 经营者看到业务结果 | Open SaaS 汇总访问、来源、注册、付费用户和收入，并通过后台任务刷新。[S5] | 已有资源聚合、图表、审计和技术指标；缺少跨资源的 SaaS 经营首页、业务事件及指标定义。单个资源的图表或 Prometheus 指标不能覆盖这条旅程。 |
| 开发者交付定制业务页面 | Wasp 有任意 React 页面、类型化 Operations、客户端 Query/Action 和实体关联的缓存失效。[S6] | 已有 Command/Query、Rust 扩展、生成客户端、自定义 React 页面及 headless controller。应补非 CRUD 成品示例和定制操作后的刷新体验，不能把它描述成完全缺少扩展能力。 |
| 构建收费 AI 功能 | Open SaaS 的任务规划示例把业务数据、模型调用、历史结果与扣点串起来；Wasp 另有流式 API 示例。[S3][S6] | 可通过现有扩展、Jobs、File、Realtime 和 Report 拼装，尚无同等完整的参考应用。资源更新 SSE 不等同于任意业务响应流。AI 示例适合应用代码，尚不需要推导出通用 Agent 平台。 |
| 开发和上线 | Wasp 提供 Fly.io/Railway 部署命令和版本相关的开发 Agent 插件。[S7] | 已有构建、生产 Compose、发布清单和晋级校验；缺少相同的平台开通向导和开发 Agent 指引。先选择一个部署入口并补版本化指引，MCP 和托管平台可单独评估。 |

Open SaaS 的支付实现包含 Stripe、Lemon Squeezy、Polar 和 Paddle 的选择入口，但需要开发者配置并删去不用的实现。
这是支付选择上的差距，不应排在“用户付费后功能是否真正开放”之前。

AppStruct 的组织、RBAC、审计、受控迁移、批量处理和已有数据库接入应继续保留。
Open SaaS 默认模板的业务数据主要按 User 归属，不能仅由其模板页面数量推断其已经具备相同的组织治理能力。[S8]

## 4. 建议的交付批次

### P0-A：可作为客户产品使用的 SaaS 外壳

- 在页面契约中区分公开、已登录个人、已选择租户三种作用域，分别支持营销布局、账户布局和工作台布局。
- 交付可定制的 Landing、Pricing、Account 与首次使用流程，并保留注册前的目标地址。
- 先打通公开页面和布局；静态预渲染作为随后独立验收项，明确公开页面、metadata、构建输出与回退路由规则。
- 营销内容、Hero 和具体业务交互放在应用拥有的 `app/web`，框架负责路由、权限、布局和配置契约。

验收：匿名访问官网和定价无需选择组织；注册后进入预期业务入口；个人账户不依赖已有租户；工作台和管理员页面继续执行各自权限。

### P0-B：收费能力连接到业务动作

- 复用现有 Stripe Billing，提供服务端权益查询和断言，并在一个真实业务动作中执行。
- 前端统一展示不可用原因、当前方案、升级入口和支付完成后的状态更新。
- 接着增加一个有边界的配额或点数场景，明确租户归属、幂等键、并发扣减、失败返还和取消订阅后的行为。
- 服务端统一约束 API、Command 和异步任务；隐藏按钮不能作为付费限制。

验收：免费用户直接调用受限接口也会被拒绝；支付事件生效后同一业务动作可用；重复事件不重复授权；并发消费不超额；失败不会永久吞掉可返还额度。
先完成这一个消费模型，再决定是否需要通用用量计费和更多支付 Provider。

### P1-A：用户文件与业务记录连起来

交付通用上传组件、进度与失败恢复、用户文件页、带授权的下载，以及记录表单中的附件字段。
优先复用当前 File Provider 与引用生命周期；直传时新增申请、完成确认和过期清理协议，校验对象归属、大小和类型。
不把已有存储对账误标为上传产品已经完成，也不默认把分片和扫描全部塞入第一版。

验收：用户可上传并关联业务记录；刷新后仍能查看和下载；另一租户不能关联或读取该文件；失败上传不会产生可用业务附件。

### P1-B：经营首页与一个完整参考应用

先交付访问、注册、首次业务成功、付费用户和收入的指标定义、数据来源、刷新时间及缺配置状态。
从跨资源仪表盘开始，复用 Jobs、Billing 和图表，不把累计收入直接命名为 MRR，也不把分析脚本默认注入所有生成应用。

建议将 SaaS Demo 做成“团队报表服务”：官网 → 注册/组织 → 上传输入 → 提交报告任务 → 查看进度 → 下载结果 → 达到免费额度 → 升级继续使用。
这能同时展示当前租户、文件、Jobs、报告和订阅的价值，并提供非 CRUD 的产品操作界面。
AI 生成可以作为同一流程的可选后端扩展示例，使用独立密钥配置；基础 Demo 不依赖外部模型才能成功。

验收：新项目按文档完成上述旅程；运营者能从看板看到注册、业务使用和付费变化；对应业务操作可从 API 再现。

### 后续批次

补版本化的 Agent 开发指引和一个部署向导；再依据目标客户选择其他支付 Provider、流式 AI 示例和更专门的页面。
看板、日历、富文本等应以选定业务场景验收，不能因为 Wasp 可以编写自定义 React 页面就算作其内置产品能力。

## 5. 真实性边界与当前代码依据

Open SaaS 的 `MessagesPage.tsx` 仍展示 under construction，`SettingsPage.tsx` 的提交仍是 TODO；它们不计为完整消息中心或账户设置。[S9]
其扣点是具体 AI 示例的应用实现，不是可直接移植的通用计费账本；本次也未据此宣称其已经覆盖通用配额、组织计费或所有失败恢复。

本项目的关键依据：

- [认证后的路由与布局](../crates/appstruct-codegen/templates/web/AppAuthenticated.tsx)：自定义页面在认证工作台内。
- [Billing 服务](../crates/appstruct-codegen/templates/backend/billing.rs)和[Billing 页面](../crates/appstruct-codegen/templates/web/billing/BillingPage.tsx)：订阅与权益展示已有实现。
- [访问规则 IR](../crates/appstruct-ir/src/lib.rs)：当前标准 access 为 public、authenticated、role、owner、any、all。
- [Activity 附件](../crates/appstruct-codegen/src/backend/activity/attachment.rs)和[File 生成器](../crates/appstruct-codegen/src/backend/file.rs)：已有文件能力及其具体入口。
- [业务扩展](business-extensions.zh-CN.md)、[Realtime](realtime.zh-CN.md)、[部署](deployment.zh-CN.md)：扩展与交付已有能力及边界。

## 6. 官方来源

以下代码链接固定到本次拉取的提交，避免后续默认分支变化影响结论：

- [S1 Open SaaS 应用入口与公开首页](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/main.wasp.ts)，以及同目录 `src/landing-page`、`src/user/AccountPage.tsx`。
- [S2 Wasp 页面、路由与布局](https://github.com/wasp-lang/wasp/blob/848143a0039abeb27b1732c7eb39510ca73c7949/web/docs/features/pages-and-routes.md)和[预渲染](https://github.com/wasp-lang/wasp/blob/848143a0039abeb27b1732c7eb39510ca73c7949/web/docs/advanced/prerendering.md)。
- [S3 Open SaaS 支付方案](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/src/payment/plans.ts)、[扣点业务动作](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/src/demo-ai-app/operations.ts)和[Provider 接口](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/src/payment/paymentProcessor.ts)。
- [S4 Open SaaS 文件操作](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/src/file-upload/operations.ts)，以及同目录 `FileUploadPage.tsx`、`fileUploading.ts`、`s3Utils.ts`。
- [S5 Open SaaS 经营统计](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/src/analytics/stats.ts)，以及 `src/admin/dashboards/analytics/AnalyticsDashboardPage.tsx`。
- [S6 Wasp Operations 与缓存失效](https://github.com/wasp-lang/wasp/blob/848143a0039abeb27b1732c7eb39510ca73c7949/web/docs/features/data/operations/actions.md)和[业务 HTTP/流式 API](https://github.com/wasp-lang/wasp/blob/848143a0039abeb27b1732c7eb39510ca73c7949/web/docs/features/apis.md)。
- [S7 Wasp 部署](https://github.com/wasp-lang/wasp/blob/848143a0039abeb27b1732c7eb39510ca73c7949/web/docs/deployment/methods/wasp-deploy/overview.md)和[Agent 集成](https://github.com/wasp-lang/wasp/blob/848143a0039abeb27b1732c7eb39510ca73c7949/web/docs/getting-started/agent-integration.md)。
- [S8 Open SaaS 默认业务模型](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/schema.prisma)。
- [S9 Open SaaS 消息占位页](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/src/admin/dashboards/messages/MessagesPage.tsx)和[设置占位实现](https://github.com/wasp-lang/open-saas/blob/c2c6285bdb14b040584505372ad9bb062a0bca11/template/app/src/admin/elements/settings/SettingsPage.tsx)。
