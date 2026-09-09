# 升级

AppStruct 技术预览发布对 CLI、编译器、生成运行时、官方模板和模块使用同步版本。应将升级视为源码、生成代码和数据库的变更，必须通过与应用发布相同的审阅。

应用已经按 [安装](installation.zh-CN.md) 跑起来之后，用本页把项目迁到更新的 CLI。数据库校验始终是显式步骤：`appstruct update` 不会连接 PostgreSQL。

## 项目升级流程

1. 提交或以其他方式备份 `appstruct.yaml`、`appstruct.lock`、`spec/`、`app/`、`migrations/` 以及 `.appstruct/schema.snapshot.json`。
2. 安装目标 AppStruct CLI 发布，并验证 `appstruct --version`。
3. 在应用检出中运行 `appstruct update`。
4. 审阅得到的 `appstruct.lock`、`generated/`、API 和 UI 差异。
5. 在生产之前，对隔离的 PostgreSQL 副本运行下方的数据库检查。

`appstruct update` 会同时获取更新锁和生成锁，将用户拥有的项目输入复制到暂存工作区，写入规范的候选锁文件，编译完整 App Spec，重新生成所有自有产物，对 Rust 运行发布级 Clippy/构建，构建 Web 应用，并运行生成后端的发布测试。提交前会再次检查源码哈希。仅锁文件和自有生成树会被替换，使用可恢复的联合日志事务。

数据库校验保持显式，因为 update 从不连接或更改数据库：

```bash
appstruct check --deny-warnings
appstruct migrate plan
appstruct generate --check
appstruct build
appstruct migrate status
```

如果 `migrate plan` 报告破坏性或需人工审阅的变更，请停止。AppStruct 故意不会把该计划转换成自动接受的迁移。将所需迁移作为应用自有的发布变更来编写并审阅。

## 当前边界

技术预览更新器会为已安装 CLI 所支持的预设规范化锁文件。它不会重写 App Spec 语法、把更新的一次性 Template 合并到用户文件、编辑迁移，或选择不受支持的未来预设版本。需要语义级 Spec 变更的发布必须在 `appstruct update` 成功之前提供已审阅的手工步骤。

可读锁文件中过期的 AppStruct 版本和预设摘要可以修复。无效的锁 TOML、不受支持的预设、校验期间源文件被更改、生成文件被修改、构建失败和测试失败都会在提交前停止更新。

## 会保留什么

- `app/`、`spec/`、迁移、schema 快照、环境文件以及 Template 自有文件永远不会被 update 覆盖。
- `generated/` 由框架拥有，仅在所有权哈希校验通过后才可能被替换。
- `generated/` 下被编辑或未知的文件会导致 update 失败关闭。
- 迁移 ID 和校验和在应用后不可变。
- schema 快照描述最近一次已接受的迁移目标，并与迁移一同流转。

中断的更新由下一次 `appstruct update` 恢复。当更新暂存、备份或日志状态仍在时，普通生成会拒绝运行。调查含糊的恢复错误时，不要手动删除这些路径。删除 `.appstruct/cache/` 是安全的，只影响速度。

## 回滚

仅当数据库仍然兼容时，才将应用和 CLI 二进制回滚到先前版本。AppStruct 不生成 down 迁移，也不会自动撤销已应用的 schema 变更。需要时使用单独审阅的前向修复或数据库还原。

对于兼容修复，创建一条新的、单调有序的迁移；永远不要编辑引入问题的那条迁移。例如，如果某次发布在所有客户行都能填充之前添加了必填的 `region` 字段，先在 App Spec 中把该字段改为 `required: false`。然后通过正常的快照事务生成前向修复：

```bash
appstruct migrate plan
appstruct migrate lint --deny-warnings
appstruct migrate dev --accept
```

审阅新生成的迁移。其有效 schema 变更应等价于：

```sql
ALTER TABLE "customers"
    ALTER COLUMN "region" DROP NOT NULL;
```

将 Spec、迁移以及更新后的 `.appstruct/schema.snapshot.json` 一起提交。针对生产快照进行测试，然后在发布中使用正常 runner，使历史、校验和、锁定和漂移检查保持权威：

```bash
appstruct migrate status
appstruct migrate apply
appstruct migrate status
```

此模式用于 schema 兼容的前向修复。当现有行需要回填时，先做应用数据迁移，再做约束修复。如果旧二进制无法在已变更的 schema 上运行，请停止流量并从已测试的数据库备份还原，而不是在事故中临时编写反向迁移。

在新发布通过健康检查和用户旅程检查之前，保留先前的后端二进制和 Web 产物。生成的后端不应针对来自更新且不兼容发布的迁移历史启动。

## 下一步

- [安装](installation.zh-CN.md)：替换 CLI 二进制
- [部署](deployment.zh-CN.md)：生产环境的 migrate status / apply
- [迁移检查](migration-lint.zh-CN.md)：升级计划含破坏性变更时先做只读检查
