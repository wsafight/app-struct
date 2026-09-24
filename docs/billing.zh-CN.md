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

密钥只在运行时配置：

- `APPSTRUCT_STRIPE_SECRET_KEY`
- `APPSTRUCT_STRIPE_WEBHOOK_SECRET`
- 每个 `price_env` 对应一个 `price_...` 值

支付信息由 Stripe Checkout 和 Customer Portal 处理。生成应用保存 Provider ID、订阅状态、周期结束时间、取消状态和套餐权益。Billing v1 暂不支持用量计费；可使用 `appstruct capabilities --format json` 查看当前 Provider 能力。
