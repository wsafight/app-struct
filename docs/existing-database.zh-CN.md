# 从已有 PostgreSQL 到第一次运行

`db pull` 读取数据库元数据，生成可审阅的领域 Spec。它会保留表名和列名，提供实际资源页面和 API 路径预览，并输出后续命令。它不会自动加入 `includes`、修改源数据库或把已有结构登记为迁移基线。

## 1. 创建应用并读取结构

```bash
appstruct init imported-admin --template dashboard --database-mode external
cd imported-admin
```

在项目 `.env` 中设置源数据库的 `DATABASE_URL`。终端中执行：

```bash
appstruct db pull --schema public --output spec/imported.yaml --review
```

逐表确认字段、关系、警告和访问策略。脚本可以使用 `--access authenticated`，或 `--access role --role admin`；这些策略要求应用启用相应的认证/RBAC 能力。默认 `--access none` 会省略访问策略，资源在补齐策略前无法编译。根据业务分别收紧 list/read/create/update/delete；例如只读资源可以把写操作限定给管理员。

复合主键、跨 schema 关系、数组、生成列及不支持的类型会产生定位到表/字段的警告。处理这些警告后，再把 `spec/imported.yaml` 加入根文件 `includes`，并检查与模板示例实体的表名冲突。AppStruct 使用 `revision` 列进行并发控制；源库没有该列时，需要审阅相应的结构变更。

## 2. 检查并生成应用

```bash
appstruct check
appstruct generate
appstruct migrate plan
appstruct migrate lint
```

先审阅计划中的业务表、修订列、模块表和索引。数据库拉取与迁移接管是两个步骤：当前 CLI 不提供自动基线登记，不能把首次产生的 `CREATE TABLE` 计划直接应用到已经存在这些表的源库。

第一次验证应用，可以把 `.env` 指向一个新的开发数据库，应用审阅后的迁移，再用 CSV 导入样本数据。这条路径可以验证字段、页面、权限和工作流；直接使用源库数据仍需先完成结构及迁移历史对齐。

## 3. 在新的开发数据库运行

将 `DATABASE_URL` 改为新的开发数据库后：

```bash
appstruct migrate dev --accept
appstruct migrate apply
appstruct doctor
appstruct dev
```

`migrate dev` 创建迁移和快照，`migrate apply` 执行待应用迁移。`doctor` 检查工具链、数据库连接和待应用迁移，并给出下一条命令。浏览器使用 `db pull` 预览的资源路径，例如 `/tasks`，API 则为 `/api/tasks/`。

注册开发账号，确认允许的列表/详情操作，以及不允许的写操作；需要角色或 owner 策略时，先配置相应角色和归属。团队视图需要租户模块和当前组织。

## 常见阻碍

| 现象 | 处理方式 |
|---|---|
| `DATABASE_URL` 缺失 | 在项目 `.env` 设置；进程环境中的同名变量优先。 |
| 访问规则验证失败 | 补齐实体或应用默认策略，并确认认证/RBAC 模块配置。 |
| 表或字段被省略 | 根据拉取警告处理不支持的键、类型和关系，重新生成另一个草稿审阅。 |
| 计划创建已有表 | 先完成已有结构的基线对齐；首次演示可使用新的开发数据库。 |
| `doctor` 显示 Migrations 失败 | `appstruct migrate status` 查看待应用文件；在开发库用 `appstruct migrate apply` 应用审阅后的迁移。 |
| 页面无权限或没有数据 | 检查当前账号、角色、组织及 owner 字段；权限作用域会影响总数、报表和导出。 |

列表默认页码分页和精确总数。导入提供字段映射、分页预览、校验、结果和失败行重试；筛选支持嵌套条件和点击统计项；Summary 的指标、分组、周期和图表配置可以随保存视图复用。
