# 本地产品旅程记录

仓库提供自愿执行的本地命令包装器，记录阶段、UTC 开始时间、耗时、退出码、是否需要帮助和预先选择的失败原因。它不会保存命令、参数、环境变量或 stdout/stderr，也不会上传数据。终端仍照常显示子命令输出。`project events` 是 Git 事件视图，不能代替这份计时记录。

在应用目录执行（将工具路径替换为 AppStruct 检出位置）：

```bash
python3 /path/to/app-struct/scripts/record-journey.py \
  --stage import -- appstruct db pull --review --output spec/imported.yaml
python3 /path/to/app-struct/scripts/record-journey.py \
  --stage second-change --help-needed --reason schema -- appstruct migrate plan
```

默认写入 `.appstruct/journeys.jsonl`，可用 `--record` 选择本地文件。每次重试保留独立记录，子命令退出码会传出；无法启动记为 127，中断记为 130。长时间运行的 `dev` 不适合直接计量“首次成功”，应记录完成首个操作所需的短命令或在观察表手工记录。人工编辑、等待和思考时间也应单独记录，命令耗时不能冒充全旅程时间。

同一试点按以下过程观察；每项记下起止时间、是否完成、失败原因、重试及人工帮助次数：

| 阶段 | 完成证据 |
| --- | --- |
| install | `appstruct --version`、`doctor` 可用 |
| import | 有存量行的数据库导入；审阅 includes、权限、revision 和 baseline |
| permissions | 两个角色及两个租户的允许/拒绝路径符合业务要求 |
| first-use | 原有记录可读，完成一次有权限的操作 |
| second-change | 新增可空字段，接受新迁移，存量行和历史校验通过 |
| build / deploy | 生产构建、`deploy verify`、smoke 和业务操作通过 |
| restore | 备份恢复到独立数据库，核对业务行和迁移状态 |
| update | 生成树更新、`template diff` 审阅、重新构建，用户代码仍保留 |

可为每位自愿参与者在本地分配匿名试点编号。记录中未收集自由文本诊断；需要备注时另建经过脱敏的观察表。只有参与者同意并人工审阅后才共享记录。比较总时间、首次完成率、重试次数和帮助次数，按阻碍排序，不以单次本地测试推断采用率、付费意愿或真实用户留存。本次仓库验证不是用户访谈结果。

## 观察状态

截至 2026-10-27，记录工具和观察流程已完成，真实用户试点尚未执行。仓库的[本地验收](optimization-progress.zh-CN.md)已验证安装、存量数据 baseline、第二次 schema 变更、原生生产构建/浏览器操作及独立数据库还原；这些结果用于工程回归，不能填入参与者的完成率或耗时。

同一批参与者应完成上表全旅程后再汇总阻碍，优先修复权限审阅、数据接管、二次变更和交付中的实际失败。Star、issue 数和内部 PR 数也不能单独证明采用率或产品方向。
