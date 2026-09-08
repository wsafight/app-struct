# 迁移检查

在接受或应用迁移前，先运行只读风险检查：

```bash
appstruct migrate lint
appstruct migrate lint --deny-warnings --format json
```

该命令会编译当前 App Spec，并与 `.appstruct/schema.snapshot.json` 比较，然后报告稳定的问题码。`AS4201` 标记破坏性 Schema 或数据变更，`AS4202` 警告现有表上的操作可能锁行，`AS4203` 标记需要人工审查 SQL 的操作，`AS4204` 捕获未提供默认值或回填的非空列新增。命令从不写入迁移、快照或数据库状态。错误会以非零退出码结束；提供 `--deny-warnings` 时，警告也会升级为错误。
