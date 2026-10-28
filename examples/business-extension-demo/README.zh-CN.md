# 业务扩展旅程示例

示例通过 public 资源便于观察事务行为，实际部署前应配置自己的认证和访问策略。

运行 `appstruct generate`，设置 `DATABASE_URL`，再执行 `appstruct migrate dev --accept` 和 `appstruct dev`。创建 Project：`{"code":"OK","name":"Normal project"}`，同一事务会插入一条 review Task。code `DENY` 被 Policy 拒绝；`FAIL` 在 after-create 中注入错误，项目和任务均回滚；`COMMIT` 在 after-commit 中注入错误，HTTP 创建仍成功且数据保留，错误在请求日志中可见。这几个 code 仅用于演示。

设置 `APPSTRUCT_LOG_FORMAT=json`，保存响应的 `X-Request-Id`，用它定位 Hook 日志。示例不发送消息，也不执行外部副作用。PostgreSQL 回归会生成此 Spec 并执行应用中的同一份扩展源码：

```bash
APPSTRUCT_E2E_DATABASE_URL=postgresql://localhost/appstruct_extension_test \
  cargo test --locked -p appstruct-codegen --test extension_journey
```

如何选择字段、Workflow、Hook/Policy、Command/Query 或自定义页面，见[业务扩展指南](../../docs/business-extensions.zh-CN.md)。
