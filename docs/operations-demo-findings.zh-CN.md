# 运维示例结论

> 状态：P0 已接受；P1 决策已记录
> 日期：2026-09-05

规范的运维示例及其 PostgreSQL/浏览器测试现在会在一个订单生命周期中同时覆盖 Tenant、RBAC、Audit、Jobs、File、Realtime、Workflow、Report 和 Activity。测试覆盖两个租户、四个角色、Workflow 字段保护、修订冲突、Report 取消/重试、File 授权、Activity，以及跨模块的租户/actor 绑定。

该组合场景暴露并修复了一个后端 Policy 组合缺陷：此前 `any` 访问规则会与租户和软删除过滤器做 OR，使 owner 规则逃出租户作用域。它也确认确定性 Job 门控和渲染器失败注入可以保留为编译期测试支持，而不需要生产 HTTP 或 CLI 控制面。

由此得到的 P1 决策是：

| 候选项 | 决策 | 证据 |
| --- | --- | --- |
| money | 接受 v1 | SupplierOffer 和 OrderLine 中重复出现金额/货币对 |
| quantity | 推迟 | 所有有用的单位都需要 Product 关系展开 |
| line items | 仅 RFC | 单独导航 OrderLine 很别扭，但原子编辑属于聚合契约 |
| role navigation | 推迟 | 当前由 Policy 派生的可见性可用；六个资源不值得引入分组语法 |
| production renderer | 仅 RFC | 浏览器渲染需要先有隔离的威胁与资源边界 |

已接受的金额契约见 `docs/business-ui-semantics.md`。推迟的聚合和渲染器边界见 `docs/aggregate-line-items-rfc.md` 与 `docs/report-renderer-adapter-rfc.md`。
