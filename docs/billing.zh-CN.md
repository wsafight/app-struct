# 生成应用收费

生成应用可以按 Spec 启用 Stripe Billing v1，为租户组织提供订阅收费：

```yaml
modules:
  billing:
    enabled: true
    provider: stripe
    capabilities:
      subscriptions: true
      trials: true
      customer_portal: true
    plans:
      - id: pro
        price_env: APPSTRUCT_STRIPE_PRICE_PRO
        trial_days: 14
        entitlements: [projects, seats]
```

编译器会生成收费表、组织所有者结账和客户门户路由、订阅页面、OpenAPI/TypeScript 客户端以及带签名校验的 Stripe Webhook。Webhook 事件 ID 会在事务提交前记录，订阅状态按 Stripe 事件时间单调更新，因此重复或乱序事件不会重复开通权益或回退旧状态。

业务命令和查询可以要求套餐权益。服务端会根据当前组织的有效订阅检查权益，客户端隐藏按钮或伪造请求都不能绕过限制：

```yaml
commands:
  GenerateReport:
    input: ReportRequest
    output: Report
    access:
      all:
        - authenticated: true
        - entitlement: reports
```

权益名称必须至少出现在一个已配置套餐中。命令和查询支持 `any`/`all` 组合；未满足访问规则返回 403，匿名请求返回 401。有效订阅必须处于 `active` 或 `trialing`，且当前周期未过期。已计划在周期结束时取消的订阅，在有效期内仍保留权益。

## 报表免费额度

报表模块提供一个有界的组织级免费额度场景；它独立于新增的月度/购买点数，不是 Stripe 用量计费：

```yaml
modules:
  report:
    entitlement: reports
    free_runs: 3
```

将这两个字段加入已有 Report 配置，并在 Billing 套餐中声明 `reports`。`free_runs` 需要 `entitlement`；只声明权益时免费额度默认为零。额度为每个组织的累计额度，不按用户或月份重置。

- 创建报表和任务时，在同一数据库事务内预占额度；运行中的报表也占用额度，不能靠并发提交超额。
- 相同请求的幂等重放返回原报表，不再次扣减。使用同一幂等键重试，不能在网络错误后生成新键。
- 成功的免费报表保留用量；取消和最终失败在更新报表状态的事务中返还。重试中的任务继续保留预占。
- 有有效付费权益时不消耗免费额度；订阅取消或过期后恢复免费额度限制。付费期间的报表不会倒扣免费额度。
- 报表保留期到期不会重置已使用额度。已返还额度的失败任务不能通过管理端重试绕过扣减，应重新提交业务动作。
- 额度耗尽返回 HTTP 402 / `QUOTA_EXCEEDED`；缺少必需权益使用 `ENTITLEMENT_REQUIRED`。Billing 概览返回 `usage`、`remaining` 和 `checkout_available`，生成界面显示升级入口。

数据库持续故障、进程崩溃和外部存储故障仍需结合 Jobs 与 Report 运维处理；本功能不宣称跨系统恰好一次或完整的崩溃对账。

密钥只在运行时配置：

- `APPSTRUCT_STRIPE_SECRET_KEY`
- `APPSTRUCT_STRIPE_WEBHOOK_SECRET`
- 每个 `price_env` 对应一个 `price_...` 值

开发环境可以完全不配置 Stripe 密钥和价格，此时免费额度仍可用，结账入口明确禁用。只配置部分凭据会启动失败；生产环境必须完整配置。支付返回页会轮询订阅，但不会仅凭 `checkout=success` 开通权益，仍以签名 Webhook 更新的状态为准。

支付信息由 Stripe Checkout 和 Customer Portal 处理。生成应用保存 Provider ID、订阅状态、周期结束时间、取消状态和套餐权益。Billing v1 暂不支持用量计费；可使用 `appstruct capabilities --format json` 查看当前 Provider 能力。
