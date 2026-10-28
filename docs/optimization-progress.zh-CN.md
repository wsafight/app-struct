# 交付优化

按 2026-09-05 授权的顺序推进。每一步都包含公开契约和相关验证，通过后再开始下一步。

- [x] 标量正确性：bigint/decimal 传输、共享 Web codec、本地日期时间往返。
  运行时测试（23）、生成标量契约测试、四个时区的 codec 测试、生成 SaaS Web 测试（32）、lint、生产构建以及范围内 Clippy 均已通过。
- [x] 授权：跨模块作用域不变量与可执行回归覆盖。
  2,112 项 PostgreSQL 真值表比较覆盖 list、member、trash 和关系作用域。已修复软删除关系目标仍可查询的问题。运维 API 与桌面/移动浏览器检查已通过，并补充了 list、聚合、CSV、Workflow、Activity 与下载隔离。
- [x] 自定义页面：共享表单与 URL 控制器、显式关系标签与导航。
  生成 Web 测试（35）、格式化、lint、TypeScript 和生产构建已通过，同时通过编译器/IR 契约、生成 Rust 编译以及运维 API/浏览器检查。
- [x] 聚合编辑：已接受的父子契约、原子 API、生成编辑器与测试。
  生成 Rust 编译、编译器校验、PostgreSQL 回滚/授权/并发、桌面/移动编辑、冲突草稿保留以及模板质量检查已通过。
- [x] 生产报表：隔离渲染适配器、打包字体/资源与生命周期检查。
  已实现 Chromium 服务、类型化渲染选项、生成部署产物、租约和事务化发布。PDF 与 PostgreSQL 租约续期/取消/过期发布检查已通过。Linux 容器隔离仍须在 CI 中运行，因为本机没有 Docker。
- [x] 运维：有界指标与数据库负载基准。
  生成后端编译、HTTP 标签隔离以及任务成功/取消/租约丢失指标已通过。20,000 行、双租户、八客户端的 PostgreSQL API 负载测试零错误通过；debug 模式下 offset/cursor/count/read/审计 CRUD 的 p95 分别为 42/9/7/8/27 ms。
- [x] 交付：可安装发布产物与可重复的首次部署验证。
  已实现安装器、同源生产 API 路由、Linux 镜像构建、健康探测和持久文件。原生发布/桌面/移动验收以及原子安装器/校验和失败检查已通过。已修复模块关闭时的 Web 构建与移动表格溢出。三个 Web 模板均通过格式化/lint/类型/测试/构建；八个 Rust 包均通过打包验证。Linux 容器隔离/部署与 Windows 安装器验收仍是 CI 要求。

公开发布是本地产物与检查可审查之后的独立发布动作。

最终本地门槛：371 项 workspace Rust 测试、rustfmt、拒绝警告的 Clippy、八个 crate 打包验证，以及 cargo-deny 0.20.2 公告检查均已通过。Workspace 测试使用 CI 性能倍数 2；隔离的编译器/生成器性能测试也通过了默认预算。运维额外通过 36 项 Web 测试、lint、格式化、TypeScript、生产构建，以及 PostgreSQL 加桌面/移动工作流回归。本地预览使用外部一次性测试数据库和 Capture 渲染；这不是 Linux Chromium 部署验收结果。

## 2026-10-12 产品工作流优化

- [x] 列表投影补齐可读币种、owner 和操作依赖；CSV 金额导出保留币种。
- [x] CSV 导入增加字段映射、每页 25 行的可编辑预览、类型校验、逐行结果和失败重试；后端按结构化 JSON 解析单元格。
- [x] 高级筛选支持嵌套 AND/OR 编辑和条件错误定位，统计项支持点击筛选与空值桶。
- [x] 批量更新复用表单控件和精确标量校验，提交前审阅值及修订号，显示逐条结果并仅重试失败记录。
- [x] 数据库拉取预览使用实际资源路径，输出后续步骤；doctor 检查待应用迁移，并补充首次运行指引。
- [x] 报表支持日/周/月 UTC 分组、保存视图复用和点击进入匹配记录；大数无法精确绘图时展示原始值表格。

管理列表继续默认页码分页和精确总数，报表配置变化保留列表所在页。当时已有数据库仍需手工完成结构与迁移历史对齐；2026-10-27 已增加显式 `migrate baseline`，见下方记录及[已有数据库指南](existing-database.zh-CN.md)。

验证：463 项 workspace Rust 测试、55 项生成 Web 测试、rustfmt、拒绝警告的 Clippy、TypeScript、ESLint、Prettier、生产构建和生成树快照均通过。真实 PostgreSQL 查询回归覆盖 JSON 导入重试、UTC 跨年分组、租户/字段权限、页码总数、游标、筛选和导出；迁移检查没有漂移。Dashboard 初始化和只读数据库拉取也已实际验证。

## 2026-10-12 生成后台 UI

统一浅色侧栏、绿色主操作、颜色变量、间距、圆角和字重。资源导航增加图标、分组和当前页面路径提示，账号与退出入口固定在侧栏底部。搜索、筛选、保存视图与列选项集中在工具面板，报表使用独立区块。

表格增加行悬停、选中高亮、枚举/布尔标签和更宽的操作列。表单、登录页、图表、CSV 预览和弹窗使用同一套视觉规范；弹窗内失败原因支持完整换行。移动端保留局部表格滚动、横向导航和单列表单，并收紧深层筛选的缩进；焦点提示和减少动画偏好继续生效。列表继续使用页码分页和精确总数。

验证：minimal/dashboard/saas 三套生成 Web 分别通过 51/54/55 项测试，以及 TypeScript、ESLint、Prettier 和生产构建；18 项生成契约测试、3 项生成树快照测试、rustfmt 和拒绝警告的 Clippy 通过。当前会话没有可用浏览器，桌面/手机截图验收未执行。

## 2026-10-13 租户切换与浏览器契约加固

- [x] 租户变化会在重新请求前重置活动的 React Query 资源，避免新请求等待期间展示其他组织的列表、审计或报表缓存。
- [x] 更新 Tenant、Audit 和 SaaS 浏览器契约，适配自定义组织对话框、限定范围的资源表单控件、当前空状态文案和审计快照重复内容。
- [x] 为租户选择和创建组织增加生成 Web 测试，覆盖旧租户迟到响应以及已认证 session 缓存保持不变。

验证：Tenant、Audit、SaaS、Mail、Jobs/Schedules/Webhooks/Realtime、File、Bulk 和 Chromium 报表
PostgreSQL/浏览器 E2E 均通过。SaaS 生成模板通过 57 项 Web 测试、TypeScript、ESLint、Prettier、生产构建和生成树快照。

## 2026-10-13 租户作用域查询缓存

- [x] 资源、审计、活动和报表的查询键加入当前租户作用域。
- [x] 租户变化后重新绑定资源和活动实时订阅，旧组织不会继续保留活动事件流。
- [x] 增加生成 Web 测试，验证不同组织使用不同缓存键，同时保留切换期间立即清空活动查询的行为。

验证：受影响模板的生成 Web framework 测试、TypeScript、ESLint、Prettier、生产构建和生成树快照均通过。

## 2026-10-27 产品旅程与当前格式清理

- [x] [已有数据库接管](existing-database.zh-CN.md)：`migrate baseline` 默认只读比较；`--accept` 仅登记结构匹配、无 seed 的标准初始迁移，不执行初始建表 SQL。漂移、自定义 SQL、迁移链及脏历史会阻断，源库结构修复仍需审阅。
- [x] [认证限流](authentication-limits.zh-CN.md)：注册在密码散列前限流，验证邮件签发与重发共享计数；跨实例 PostgreSQL 计数和 `429 / Retry-After: 60` 已验证。来源维度由可信部署网关定义。
- [x] [品牌与主题](web-customization.zh-CN.md)：三个模板提供用户拥有的 `app/web/setup.ts`，支持标题、Logo、色板、主题和常用文案，generate/update 保留该文件；尚非完整 i18n。
- [x] [数据生命周期](data-lifecycle.zh-CN.md)：审计入库前脱敏、可选留存与显式分批清理；文件业务引用、保留期回收、内置引用保护及存储失败重试。对象删除期间持有元数据行锁，防止并发回收误删复用对象。存储删除不能随数据库回滚，无元数据孤儿仍需对账。
- [x] [业务扩展](business-extensions.zh-CN.md)：可运行示例覆盖 Policy 拒绝、事务内 Hook 回滚、after-commit 失败及 request ID 日志关联；[升级指南](upgrading.zh-CN.md)补充只读 `template diff` 和人工合并验证流程。
- [x] [前端校验](headless-controller.zh-CN.md)：移除生成 Web 的直接 Zod 依赖，TanStack Form 复用 `valueError`；保留 UUID、精确标量、金额/币种配对、提交转换和冲突草稿保护。工具链传递依赖不作为清理目标。
- [x] [当前格式契约](compatibility.zh-CN.md)：只读写 IR 17、snapshot 3、layout 2，删除旧格式补齐、别名和布局推断；保留版本匹配、所有权 hash、迁移历史/校验和及事务崩溃恢复。现有开发目录需备份后按当前模板重建。
- [x] [产品旅程记录](product-journeys.zh-CN.md)：自愿本地记录阶段、耗时、退出码、帮助和失败原因，不保存命令参数、环境或输出，不上传；工具回归已纳入 CI。

### 实际本地验收

- minimal/dashboard/saas 分别通过 57/60/63 项 Web 测试，以及类型、lint、格式和生产构建。独立目录 frozen install 复用了本机 pnpm store，未验证纯网络冷安装。
- 实际 `appstruct build`、`deploy verify`、原生 release 后端和生产 Web 的 smoke、桌面/移动修订安全 CRUD 均通过；用户 setup 的标题、暗色主题、色板和表单文案经过浏览器验证。
- 独立 PostgreSQL 验证存量行保留、第二次可空字段迁移及真实 `pg_dump/pg_restore`。还原后 applied 2 / pending 0 / drift none；专用测试集群已停止并删除。
- CLI、Contracts、IR、Schema、Compiler、Codegen 回归，真实 PostgreSQL Migrate 全量回归、生命周期/扩展/并发回收、生成树 golden 和范围内 Clippy 均通过。当前 composition-root 和本地/S3 文件生成产物编译通过。
- macOS 本地归档安装、替换、损坏归档拒绝及原二进制保留通过；旅程记录器、rustfmt、文档构建与链接检查通过。

### 尚待对应环境执行

- [ ] Linux Docker/Compose 首次部署及容器隔离的本次运行结果。
- [ ] Windows x64 安装器的本次运行结果。已补入完整 CI，无需创建版本标签即可运行。
- [ ] 自愿真实用户的完整旅程观察；没有采用率、付费意愿或留存结论。

平台执行入口与状态见[仓库质量门禁](continuous-integration.zh-CN.md)，后续场景投入见[产品路线图](next-product-roadmap.zh-CN.md)。已有测试与 workflow 不能代替实际平台结果或真实用户反馈。

## 2026-10-28 六项产品优化与 0.2.0 候选

- [x] [已有数据库预检](existing-database.zh-CN.md)：在内存中编译 include 提案，只读比较实际结构，报告缺少 revision、遗漏表和编译诊断；不将结构匹配等同于迁移历史已接受。
- [x] [CSV 导入恢复](bulk-operations.zh-CN.md)：不可变 UUID 批次、内容摘要和结果账本与业务数据同事务提交；并发重试只写一次，丢失响应后查询结果。同批次更换内容被拒绝，权限按资源、actor、租户隔离。
- [x] [编辑冲突合并](headless-controller.zh-CN.md)：比较基线、服务器和草稿；自动合并单侧变更，冲突逐项选择，应用后显式保存并固定对照修订号。日期时间对比与选择保留微秒。
- [x] [权限预览](access-preview.zh-CN.md)：CLI 解释 CRUD、字段、owner、租户和聚合子项限制；自定义 Rust Policy 明确标为未执行。
- [x] [中文界面与格式](web-customization.zh-CN.md)：启动时语言和字段显示名，标准流程文案，以及数字、金额、日期和显示时区格式统一。原生日期时间输入兼容毫秒精度，未改动时间保留原始微秒；修正按钮宽度与状态样式。
- [x] [条件与跨字段校验](form-validation.zh-CN.md)：类型化 `required_when`、严格 `greater_than`；前端共享 `valueError`，服务端在 Hook 后校验完整候选记录，覆盖部分更新、导入、批量、聚合和 Workflow。显式 null 可正确清空可空字段，没有恢复直接 Zod 依赖；新建表单会提交未点击的必填 false 复选框。

版本元数据已准备为 0.2.0，IR 18 / runtime API 5 / snapshot 3 / layout 2。[候选发布说明](release-0.2.0.zh-CN.md)记录迁移边界。核实 crates.io 后确认 0.1.0 已于 2026-08-29 发布，不能继续假设没有历史发布；0.2.0 的格式变化属于破坏性变更。此次未推送、打标签、发布 crate 或创建 Release。

### 本地验证

- minimal/dashboard/saas 分别通过 64/67/70 项 Web 测试，frozen install、生产依赖 audit、Prettier、ESLint、TypeScript 和生产构建均通过。安装使用了本机 pnpm store。
- 真实生成界面的 Chromium 桌面/390px 手机检查通过：中文、字段名、微秒时间输入、冲突对照与显式保存，CSV 丢失响应后的批次结果恢复，以及新建时未点击的必填 false 和数量 0。日期时间 codec 还通过 UTC、纽约、柏林和上海四个时区的回归。此 UI 检查拦截了 HTTP 响应；服务端事务、并发及隔离由独立真实 PostgreSQL 集成测试验证。
- 十个官方 crate 均通过实际打包和验证编译；macOS arm64 release 归档、校验和、含空格路径安装、替换、损坏归档拒绝及原二进制保留均通过。
- workspace Clippy（拒绝警告）、rustfmt、cargo-deny 公告/许可证/来源检查、旅程记录器、actionlint 和全部 shell 语法检查通过。公告修复涉及 ctutils 0.4.4、Rustls 0.23.46、rustls-webpki 0.103.16、webpki-root-certs 1.0.10 及 Tokio 1.53.3。
- 文档站检查无错误/警告，54 页构建、搜索和内部链接检查通过。
- 473 项 workspace Rust 测试通过，零失败、零忽略；包含真实 PostgreSQL 迁移、接管、批次与表单规则集成测试。性能检查使用 CI 倍数 2，最新生成树 golden 另行验证通过。

专用 PostgreSQL 测试集群和浏览器预览已停止，临时项目已删除；日志、截图与本机候选归档保留在被忽略的 `target/` 下。

Linux Docker/Compose、渲染隔离、Windows 安装器及完整五平台发布矩阵仍须在候选 commit 上实际执行；自愿真实用户观察也尚未执行。不能把本机结果视为这些环境的验收。
