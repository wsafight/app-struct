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

确认写入草稿前，交互审阅会执行只读接管预检；非交互模式也在文本/JSON 输出中提供 `preflight`。预检在内存中加入候选领域和 includes，检查与当前项目的编译冲突，并报告未导入表、缺少的 revision、模块表及其他结构差异。JSON 的 `include` 是需要人工确认的配置建议，不修改根配置。`structure_matches` 只表示结构匹配；预检不检查迁移历史，也不执行结构 SQL 或 baseline 登记。仍须完成下方的 `migrate baseline` 验收。

## 2. 检查并生成应用

```bash
appstruct check
appstruct generate
appstruct migrate plan
appstruct migrate lint
```

先审阅计划中的业务表、修订列、模块表和索引。数据库拉取与迁移接管是两个步骤：需要显式运行基线登记，不能把首次产生的 `CREATE TABLE` 计划直接应用到已经存在这些表的源库。

### 接管现有数据库

先备份源库，并在副本上演练。初始计划中的 `revision`、Auth 等模块表、默认值、约束和索引都需要与目标结构对齐。`migrate baseline` 不执行结构修复；应审阅初始 SQL，提取缺失结构所需的变更，通过原有数据库变更流程应用。不要对源库执行整份初始建表 SQL。

```bash
appstruct migrate baseline          # 只读比较；差异会阻止接管
appstruct migrate baseline --accept # 匹配后创建 migration/snapshot 并登记历史
appstruct migrate status
appstruct doctor
appstruct dev
```

默认预览不写项目文件或数据库历史。`--accept` 只登记初始迁移，保留业务数据，不执行其中的 SQL；后续可在新空库用这份迁移重建相同结构。重复登记是幂等的。若文件已创建而数据库登记失败，保留文件，修复原因后重试同一条命令。

此流程要求当前 schema 中所有受管理表匹配，不能忽略未导入的额外表/列/索引。它仅接受一份未修改的标准初始迁移及匹配快照，不会跳过任意 SQL、已有迁移链或脏历史。Spec 暂时不能包含 seed，避免把尚未插入的数据标记为已应用；接管后如需 seed，应单独审阅数据并创建后续迁移。复杂约束、触发器和 RLS 等不属于 Spec 完整建模范围，仍需单独审阅其行为与生成应用的兼容性。

验收时读取现有记录并执行允许的更新，检查 `revision` 并发控制。再添加一个可空字段，运行 `migrate dev --accept`、`migrate status`，确认后续迁移与原有数据都正常。

### 使用新的开发数据库

第一次验证应用，也可以把 `.env` 指向一个新的开发数据库，应用审阅后的迁移，再用 CSV 导入样本数据。这条路径可以验证字段、页面、权限和工作流；也可以按上一节接管已对齐的源库副本。

## 3. 在新的开发数据库运行

将 `DATABASE_URL` 改为新的开发数据库后：

```bash
appstruct migrate dev --accept
appstruct migrate apply
appstruct doctor
appstruct dev
```

`migrate dev` 创建迁移和快照，`migrate apply` 执行待应用迁移。`doctor` 检查工具链、数据库连接和待应用迁移，并给出下一条命令。浏览器使用 `db pull` 预览的资源路径，例如 `/tasks`，API 则为 `/api/tasks/`。

注册开发账号，确认允许的列表/详情操作，以及不允许的写操作；需要角色或 owner 策略时，先配置相应角色和归属。团队视图需要租户模块和当前组织。注册与邮件入口的限制和重试规则见[认证与邮件限流](authentication-limits.zh-CN.md)。

## 常见阻碍

| 现象 | 处理方式 |
|---|---|
| `DATABASE_URL` 缺失 | 在项目 `.env` 设置；进程环境中的同名变量优先。 |
| 访问规则验证失败 | 补齐实体或应用默认策略，并确认认证/RBAC 模块配置。 |
| 表或字段被省略 | 根据拉取警告处理不支持的键、类型和关系，重新生成另一个草稿审阅。 |
| 计划创建已有表 | 对齐结构后用 `migrate baseline` 预览，再显式登记；不要应用整份初始建表 SQL。 |
| `doctor` 显示 Migrations 失败 | `appstruct migrate status` 查看待应用文件；在开发库用 `appstruct migrate apply` 应用审阅后的迁移。 |
| 页面无权限或没有数据 | 检查当前账号、角色、组织及 owner 字段；权限作用域会影响总数、报表和导出。 |

列表默认页码分页和精确总数。导入提供字段映射、分页预览、校验、结果和失败行重试；筛选支持嵌套条件和点击统计项；Summary 的指标、分组、周期和图表配置可以随保存视图复用。
