# 复杂业务扩展与排错

优先选择能够表达业务约束、且容易审阅的现有入口。不要为每条业务规则增加一层 YAML 语法。

| 需求 | 入口 | 应用负责什么 |
| --- | --- | --- |
| 长度、范围、枚举、关系、角色字段读写 | 标准字段与声明式 access | 模型、索引及实际角色验收 |
| 单记录状态和有权限的状态转换 | Workflow | 状态/转换设计、输入及并发冲突处理 |
| 规范化输入、事务内联动、按记录内容决定授权 | Hook / Policy | 使用当前 context 的租户和事务，避免裸查询绕过作用域 |
| 独立业务动作或定制读取 | Command / Query handler | 声明式操作授权之外的记录授权、查询作用域和事务边界 |
| 父子行一起保存 | aggregate editing | 明细权限、修订号、服务端原子性 |
| 定制交互、跨资源组合 | 自定义 React 页面及 headless controller | 页面路由、缓存、加载/错误/冲突体验；服务端仍负责授权 |

Workflow 是单实体状态机，不能把多个服务的外部副作用当成可回滚数据库事务。多步骤外部业务需应用自己的幂等协议、持久任务和补偿设计。计算字段、跨字段校验和动态字段权限超出默认声明能力时，先用服务端扩展落实，再决定是否值得成为通用 Spec 契约。

## 可运行的事务与诊断示例

[业务扩展示例](../examples/business-extension-demo/README.zh-CN.md) 在创建 Project 时用 `after_create` 插入一条 Task，使用的正是 `ctx.database()`，与项目共用事务。`ProjectPolicy` 拒绝 code `DENY`；code `FAIL` 在插入任务后注入错误，项目和任务都回滚；code `COMMIT` 在提交后注入错误，HTTP 仍创建成功，数据库行保留，错误记入日志。

```bash
cd examples/business-extension-demo
appstruct generate
# 在 .env 中设置独立开发数据库 DATABASE_URL
appstruct migrate dev --accept
APPSTRUCT_LOG_FORMAT=json appstruct dev
```

在界面或 API 中分别创建上述 code；正常路径使用 `OK`。示例为了便于观察采用 public access，并保留几个故障注入码，不能直接作为生产权限方案。其 PostgreSQL 回归会执行示例中的同一份 Rust 源码，同时检查 Policy 拒绝、事务回滚、提交后行为和请求日志关联。

排错按这条路径推进：

1. 保存响应状态、错误码和 `X-Request-Id`；记录脱敏后的复现步骤，不复制密码、cookie 或完整环境文件。
2. 用 `APPSTRUCT_LOG_FORMAT=json` 和适当 `RUST_LOG` 启动，按 request ID 找同一 HTTP span 中的 Hook 日志。示例显式记录 `Project.after_create`。
3. 分辨 validate、事务内 Hook、Policy 与 after-commit 阶段。`before_validate_*` 在生成写入事务之前执行，应只规范化/验证输入；事务内写入使用 `ctx.database()`。`after_commit` 的失败不能撤销提交，不应把返回错误理解成业务未发生。
4. 对照当前租户、actor、目标记录、ETag 修订号及数据库行确认结果。自定义 SQL 必须显式约束租户；`RequestContext` 不会自动改写任意 SQL。
5. 在独立数据库添加与真实故障对应的回归，然后运行 `appstruct check`、`generate --check`、构建和相关旅程。

需要可靠发送邮件/Webhook或外部通知时，在业务事务内登记 Jobs/outbox 工作；不要假定 after-commit 网络调用会自动重试。自定义 Command/Query handler 取得的是请求 context，应用应显式建立其需要的事务，并继续使用绑定该事务的 context。

## 代码所有权与模块

`app/backend`、`app/web`、Spec 和迁移由应用拥有，生成与更新保留它们；`generated/` 由框架管理，直接编辑会触发所有权检查。生成 trait 改动时，应用通过编译错误更新扩展签名，再验收行为。

远程模块包提供已签名的静态 Artifact 和元数据，不能仅靠安装就注册任意第三方 Rust 运行时。应用自有 Rust 在 composition root 编译并通过 `AppExtensions` 注册；官方运行时模块由编译器内置生成和启动。三者有不同的依赖、迁移和卸载责任，详见[模块注册表](module-registry.zh-CN.md)。现阶段不提供动态库或 WASM 执行插件契约。

继续阅读：[Workflow](workflows.zh-CN.md)、[无头控制器](headless-controller.zh-CN.md)、[指标与日志](observability.zh-CN.md)、[升级](upgrading.zh-CN.md)。
