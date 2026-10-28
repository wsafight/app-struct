# AppStruct 0.2.0

发布说明：2026-10-28。

本版本集中优化从已有 PostgreSQL 接入到日常后台操作：

- `db pull` 提供只读接管预检，结构修复和 baseline 接受仍显式执行。
- CSV 批次幂等与业务写入同事务提交，丢失响应后可查询已提交结果。
- 编辑冲突对比基线、服务器和草稿，应用选择后，使用对照修订号显式保存。
- `access preview` 解释资源、字段、owner 和租户声明，不执行自定义 Policy。
- 启动时中英文标准界面、业务字段显示名，以及统一的精确数字、日期和显示时区。
- `required_when` 与 `greater_than` 类型化规则同时用于标准表单和服务端完整候选记录；显式 null 更新能正确清空可空字段。
- 修复依赖公告：生成后端与 workspace 使用 Tokio 1.53.3，锁定已修复的 TLS 和常量时间运算依赖。

同时包含此前完成的认证限流、品牌主题、数据生命周期、扩展示例、模板 diff 和前端直接 Zod 清理。官方 crate 与生成运行库元数据统一为 0.2.0。当前契约为 IR 18、runtime API 5、snapshot 3、project layout 2。

0.1.0 已在 crates.io 发布（[编译器记录](https://crates.io/crates/appstruct-compiler/0.1.0)，2026-08-29）；截至本次核查，GitHub 没有 Release 或标签。0.2.0 包含破坏性格式变更，不提供历史 IR、snapshot 或项目布局适配层。升级须先备份，从 Spec 重建 IR，并按[升级指南](upgrading.zh-CN.md)在独立目录审阅、重建旧格式项目；普通 update 不会转换旧格式。已有数据库需审阅 `_appstruct_import_batches` 的迁移，并对其他系统表、列和索引运行完整结构比较；保留数据、已应用迁移及其校验和，不能把旧库当成新库执行初始建表。

本地结果维护在[交付优化](optimization-progress.zh-CN.md)。完整 CI 覆盖 Linux 容器与渲染隔离、Windows 安装器、所有 PostgreSQL 套件、模板和打包；Release 流程在五种原生平台构建并测试安装器后产出归档与校验和。运行时语言切换、任意跨记录校验和导入账本自动过期不在本版本范围。

二进制和校验和见 [GitHub Release](https://github.com/wsafight/app-struct/releases/tag/v0.2.0)，Rust 包见 [crates.io](https://crates.io/crates/appstruct-cli/0.2.0)。维护者按[发布流程](releasing.zh-CN.md)执行发布。
