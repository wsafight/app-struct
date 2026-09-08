export const groups = [
  {id: 'start', number: '01', label: {en: 'Start here', zh: '开始使用'}},
  {id: 'delivery', number: '02', label: {en: 'Build and delivery', zh: '构建与交付'}},
  {id: 'data', number: '03', label: {en: 'Data modeling', zh: '数据建模'}},
  {id: 'application', number: '04', label: {en: 'Application design', zh: '应用设计'}},
  {id: 'platform', number: '05', label: {en: 'Platform modules', zh: '平台模块'}},
  {id: 'operations', number: '06', label: {en: 'Operations', zh: '生产运维'}},
  {id: 'project', number: '07', label: {en: 'Project records', zh: '项目记录'}},
];

function zhSource(source) {
  return source.replace(/(\.md)$/, '.zh-CN.md');
}

export const pages = [
  {source:'README.md', translation:'README.zh-CN.md', slug:'start/overview', group:'start', title:{en:'Overview and quick start', zh:'概览与快速开始'}, description:{en:'Build the CLI, create a project, and understand the generated stack.', zh:'构建 CLI、创建项目并了解生成的完整技术栈。'}},
  {source:'docs/installation.md', slug:'start/installation', group:'start', title:{en:'Installation', zh:'安装'}, description:{en:'Install AppStruct from source or a verified release artifact.', zh:'从源码或校验过的发布产物安装 AppStruct。'}},
  {source:'docs/upgrading.md', slug:'start/upgrading', group:'start', title:{en:'Upgrading', zh:'升级'}, description:{en:'Upgrade the CLI, project lock, generated artifacts, and database safely.', zh:'安全升级 CLI、项目锁、生成物与数据库。'}},

  {source:'docs/deployment.md', slug:'delivery/deployment', group:'delivery', title:{en:'Deployment', zh:'部署'}, description:{en:'Build immutable artifacts and run migrations as an explicit release step.', zh:'构建不可变产物，并将迁移作为显式发布步骤。'}},
  {source:'docs/releasing.md', slug:'delivery/releasing', group:'delivery', title:{en:'Releasing AppStruct', zh:'发布 AppStruct'}, description:{en:'Cut and verify framework releases with reproducible checks.', zh:'通过可复现检查完成框架版本发布。'}},
  {source:'docs/migration-lint.md', slug:'delivery/migration-lint', group:'delivery', title:{en:'Migration lint', zh:'迁移检查'}, description:{en:'Review destructive, locking, and unsafe schema changes before shipping.', zh:'在交付前审查破坏性、锁表和不安全的 Schema 变更。'}},
  {source:'docs/module-registry.md', slug:'delivery/module-registry', group:'delivery', title:{en:'Module registry', zh:'模块注册表'}, description:{en:'Resolve signed remote modules with locked versions and digests.', zh:'使用锁定版本与摘要解析签名远程模块。'}},
  {source:'docs/optimization-progress.md', slug:'delivery/optimization', group:'delivery', title:{en:'Delivery optimization', zh:'交付优化'}, description:{en:'Track compiler, generator, and build-time improvements.', zh:'记录编译器、生成器与构建耗时优化。'}},

  {source:'docs/data-querying.md', slug:'data/querying', group:'data', title:{en:'Resource queries', zh:'资源查询'}, description:{en:'Pagination, filters, relations, aggregates, and access-aware query contracts.', zh:'分页、筛选、关系、聚合与权限感知查询契约。'}},
  {source:'docs/scalar-values.md', slug:'data/scalar-values', group:'data', title:{en:'Scalar values', zh:'标量值'}, description:{en:'Lossless values and datetime controls across Rust, JSON, and TypeScript.', zh:'在 Rust、JSON 与 TypeScript 间保持无损值和日期时间控制。'}},
  {source:'docs/schema-indexes.md', slug:'data/indexes', group:'data', title:{en:'Schema indexes', zh:'Schema 索引'}, description:{en:'Declare ordered composite and partial PostgreSQL indexes.', zh:'声明有序复合索引与 PostgreSQL 部分索引。'}},
  {source:'docs/seeding.md', slug:'data/seeding', group:'data', title:{en:'Seed data', zh:'Seed 数据'}, description:{en:'Create reviewable and idempotent seed rows from the App Spec.', zh:'从 App Spec 创建可审查、幂等的 Seed 数据。'}},
  {source:'docs/aggregate-line-items-rfc.md', slug:'data/aggregate-line-items', group:'data', title:{en:'Aggregate line items', zh:'聚合明细行'}, description:{en:'Model bounded aggregate children and transactional parent updates.', zh:'建模受限聚合子项与事务化父记录更新。'}},
  {source:'docs/relation-display.md', slug:'data/relation-display', group:'data', title:{en:'Relation display', zh:'关系展示'}, description:{en:'Render relation labels and lookup controls in generated applications.', zh:'在生成应用中渲染关系标签与查找控件。'}},
  {source:'docs/soft-delete.md', slug:'data/soft-delete', group:'data', title:{en:'Soft delete and history', zh:'软删除与历史'}, description:{en:'Archive, restore, and audit records without losing revision semantics.', zh:'在保留修订语义的前提下归档、恢复并审计记录。'}},
  {source:'docs/saved-views.md', slug:'data/saved-views', group:'data', title:{en:'Saved views', zh:'保存视图'}, description:{en:'Persist private list views and share query-state URLs.', zh:'保存私有列表视图并分享查询状态 URL。'}},

  {source:'docs/headless-controller.md', slug:'application/headless-controller', group:'application', title:{en:'Headless controllers', zh:'无头控制器'}, description:{en:'Keep generated resource behavior independent from visual components.', zh:'让生成资源行为独立于具体视觉组件。'}},
  {source:'docs/business-ui-semantics.md', slug:'application/ui-semantics', group:'application', title:{en:'Business UI semantics', zh:'业务 UI 语义'}, description:{en:'Compile business intent into dense, consistent management interfaces.', zh:'将业务意图编译为紧凑一致的管理界面。'}},
  {source:'docs/bulk-operations.md', slug:'application/bulk-operations', group:'application', title:{en:'Bulk operations', zh:'批量操作'}, description:{en:'Bulk update, delete, import, and export with partial-failure reporting.', zh:'支持带部分失败明细的批量更新、删除、导入与导出。'}},
  {source:'docs/workflows.md', slug:'application/workflows', group:'application', title:{en:'Entity workflows', zh:'实体工作流'}, description:{en:'Declare guarded state transitions and workflow actions.', zh:'声明带守卫条件的状态转换与工作流操作。'}},
  {source:'docs/reports.md', slug:'application/reports', group:'application', title:{en:'Reports', zh:'报表'}, description:{en:'Define report inputs, datasets, renderers, and access policies.', zh:'定义报表输入、数据集、渲染器与访问策略。'}},
  {source:'docs/activity.md', slug:'application/activity', group:'application', title:{en:'Record activity', zh:'记录动态'}, description:{en:'Expose authorized audit history alongside business records.', zh:'在业务记录旁展示经过授权的审计历史。'}},

  {source:'docs/email-verification.md', slug:'platform/email-verification', group:'platform', title:{en:'Email verification', zh:'邮箱验证'}, description:{en:'Verify addresses with expiring, single-use tokens and captured mail.', zh:'使用限时一次性 Token 与邮件捕获完成邮箱验证。'}},
  {source:'docs/organization-invitations.md', slug:'platform/invitations', group:'platform', title:{en:'Organization invitations', zh:'组织邀请'}, description:{en:'Invite members into tenant-scoped organizations safely.', zh:'安全邀请成员加入租户隔离的组织。'}},
  {source:'docs/oauth-oidc.md', slug:'platform/oauth-oidc', group:'platform', title:{en:'OAuth and OIDC', zh:'OAuth 与 OIDC'}, description:{en:'Connect external identities while preserving AppStruct sessions.', zh:'接入外部身份并保持 AppStruct 会话模型。'}},
  {source:'docs/personal-api-tokens.md', slug:'platform/api-tokens', group:'platform', title:{en:'Personal API tokens', zh:'个人 API Token'}, description:{en:'Create scoped, revocable credentials for API and automation access.', zh:'创建可限定范围、可撤销的 API 与自动化凭据。'}},
  {source:'docs/realtime.md', slug:'platform/realtime', group:'platform', title:{en:'Realtime and presence', zh:'实时事件与在线状态'}, description:{en:'Deliver policy-filtered events, presence, and optional edit leases.', zh:'交付经 Policy 裁剪的事件、在线状态与可选编辑租约。'}},
  {source:'docs/schedules.md', slug:'platform/schedules', group:'platform', title:{en:'Schedules', zh:'定时任务'}, description:{en:'Run interval schedules with explicit missed-run semantics.', zh:'使用明确的漏跑语义执行固定间隔任务。'}},
  {source:'docs/webhooks.md', slug:'platform/webhooks', group:'platform', title:{en:'Signed webhooks', zh:'签名 Webhook'}, description:{en:'Deliver signed events through a durable, retryable outbox.', zh:'通过持久、可重试的 Outbox 交付签名事件。'}},

  {source:'docs/admin-console.md', slug:'operations/admin-console', group:'operations', title:{en:'Administration console', zh:'运维控制台'}, description:{en:'Inspect health, queues, deliveries, and guarded recovery actions.', zh:'查看健康、队列、投递与受保护的恢复操作。'}},
  {source:'docs/observability.md', slug:'operations/observability', group:'operations', title:{en:'Metrics and workloads', zh:'指标与数据库负载'}, description:{en:'Measure API latency, database work, and bounded operational labels.', zh:'测量 API 延迟、数据库工作量与有界运维标签。'}},
  {source:'docs/report-renderer.md', slug:'operations/report-renderer', group:'operations', title:{en:'Chromium report renderer', zh:'Chromium 报表渲染器'}, description:{en:'Render production reports in an isolated Chromium service.', zh:'在隔离的 Chromium 服务中渲染生产报表。'}},
  {source:'docs/report-renderer-adapter-rfc.md', slug:'operations/report-adapter-rfc', group:'operations', title:{en:'Report renderer adapter RFC', zh:'报表渲染适配器 RFC'}, description:{en:'Define the production adapter boundary for external renderers.', zh:'定义外部渲染器的生产适配边界。'}},
  {source:'docs/operations-demo-findings.md', slug:'operations/demo-findings', group:'operations', title:{en:'Operations demo findings', zh:'运维示例结论'}, description:{en:'Document end-to-end findings from the operations reference app.', zh:'记录运维参考应用的端到端验证结论。'}},

  {source:'docs/next-product-roadmap.md', slug:'project/roadmap', group:'project', title:{en:'Product roadmap', zh:'产品路线图'}, description:{en:'Prioritize the next product milestones and their acceptance boundaries.', zh:'确定下一阶段产品里程碑及其验收边界。'}},
  {source:'PRODUCT.en.md', translation:'PRODUCT.md', slug:'project/product', group:'project', title:{en:'Product baseline', zh:'产品基线'}, description:{en:'The problem, product principles, target users, and capability scope.', zh:'产品问题、原则、目标用户与能力范围。'}},
  {source:'TECHNICAL_DESIGN.en.md', translation:'TECHNICAL_DESIGN.md', slug:'project/technical-design', group:'project', title:{en:'Technical design', zh:'技术设计'}, description:{en:'Architecture, crate ownership, protocols, and delivery sequence.', zh:'架构、Crate 职责、核心协议与交付顺序。'}},
].map(page => ({...page, translation: page.translation || zhSource(page.source)}));
