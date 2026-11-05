# SaaS 产品闭环

`examples/saas-demo` 在现有 SaaS preset 上增加公开官网、定价、账户、团队报表工作台、用户文件、免费额度和经营概览。营销和业务界面位于应用拥有的 `app/web`，不写进框架的通用资源页。

`appstruct new --template saas` 仍生成基础 Starter；产品 Demo 有意显式扩展模块，不改变 preset v1，也不强制每个新应用启用收费。

## 页面作用域与布局

```yaml
pages:
  Landing:
    path: /
    scope: public
    component: LandingPage
  Profile:
    path: account
    scope: account
    component: ProfilePage
  Studio:
    path: studio
    scope: workspace
    component: StudioPage
```

- `public` 不要求登录或组织；只有公开页可以使用根路径 `/`。
- `account` 需要登录，不要求已有组织。框架默认生成 `/account`；可以用同作用域的同路径页面覆盖。Tenant 应用另外生成 `/onboarding`。
- `workspace` 为省略时的默认值，保持旧页面语义；启用 Tenant 时必须先选择或创建组织。
- 有公开根页时，工作区入口移到 `/workspace`；否则仍为 `/`。登录/注册切换和刷新保留 `next` 的本地路径、查询参数与 hash，拒绝外部地址和认证循环。
- 页面不能覆盖资源路径及框架保留路径。`account` 路径只供账户作用域使用。

应用注册表可选配置 `layouts.public`、`layouts.account`、`layouts.workspace`，组件接收 `scope`、`pages`、`resources` 和 `children`，必须渲染 `children`。目前自定义注册表入口由声明的页面或字段组件触发；仅定义布局但没有页面/字段声明不会自动导入。Demo 的 `pnpm saas:build` 另外预渲染官网和定价 HTML；这不是框架通用 SSR。

## 用户文件

Auth + File 应用生成 `/files` 和以下带认证 API：

- `GET /api/files?page=1&page_size=25`：分页列表，页码 1–10000，每页 1–100。
- `POST /api/files?name=...`：请求体为文件内容，`Content-Type` 为文件 MIME，使用 CSRF 和当前组织请求头。
- `GET /api/files/{id}/download`：按附件下载，私有且不缓存。
- `DELETE /api/files/{id}`：需要 CSRF；仍有业务引用返回 409。

启用 Tenant 时文件属于当前组织，否则属于当前用户；服务端同时检查文件 ID、对象键作用域和租户，而不是信任客户端传入的对象键。报表输出文件不进入用户上传库。上传复用 File 的大小、MIME、Local/S3 和引用回收规则；界面提供进度、取消、分页、下载和删除确认。

新上传界面使用申请 → 上传 → 完成协议：Local 经后端代理，S3 对暂存键预签名直传，完成时再检查大小与 MIME 并复制到不可变发布键。会话 15 分钟过期，完成可重放，支持取消。资源详情页提供附件集合、替换和解绑；仍无声明式 `type: file` 字段、分片或病毒扫描。附件保留原文件库作用域，不会自动缩小为记录级私密文件。报表 Demo 读取授权下载的文本/JSON，并把内容固化为加密报表快照。

## 额度、支付与报表

每个组织有累计 3 次免费成功报表额度，排队和运行时预占；取消和最终失败返还。Pro 的 `reports` 权益允许继续提交。服务端扣减、并发和幂等规则见[收费文档](billing.zh-CN.md)。

工作台串联上传 → 选择输入 → 提交 → 查看进度 → 下载。开发环境默认 Capture Renderer，用于闭环验证，不代表完整排版或生产 PDF 效果。生产 Chromium 配置、加密密钥和运行边界见[报表文档](report-renderer.zh-CN.md)。

## 经营概览

`/admin/business` 和 `GET /api/admin/business` 仅供全局 `admin`，API 不依赖组织选择。每分钟刷新，并返回生成时间、每项定义、数据来源、统计窗口和不可用原因：

| 指标 | 口径 |
| --- | --- |
| 注册账户 | 当前仍存在的 Auth 账户 |
| 组织 | 当前仍存在的 Tenant 组织 |
| 成功报表 | 保留期内的成功记录，不是终身使用次数 |
| 已激活账户 | 保留期内至少完成一份报表的去重账户 |
| 付费组织 | 各组织最新订阅为 active 且周期未过期；排除 trialing |
| 访问 | 最近 30 天经用户同意的第一方访问事件，按访客、UTC 日期和页面去重；不是原始 PV |
| 结算收入 | 仍不可用，不能用订阅数或支付捕获金额冒充银行结算 |

不注入第三方追踪脚本。新增 `/admin/insights` 展示访问/注册/激活窗口数、完整观察的 D7 激活留存，以及按币种分开的支付捕获/退款金额；不将它们称为归因转化、MRR 或银行结算。

## 验证边界

`product_journey` 在真实 PostgreSQL 上验证 12 路并发仅接受 3 次、幂等重放、取消/最终失败返还、订阅变化、跨组织文件拒绝、引用保护以及经营端权限。浏览器测试 `tests/e2e/saas-product.spec.ts` 使用模拟 API 验证页面导航和上传/报表/升级交互，不代表真实 Stripe 支付验收。

本轮 P0–P2 的能力、配置、接口及验收命令见[产品能力交付](saas-product-capabilities.zh-CN.md)。
