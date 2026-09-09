# 迁移检查

在接受或应用迁移前运行此只读检查。它从不写入迁移、快照或数据库状态。

```bash
appstruct migrate lint
appstruct migrate lint --deny-warnings --format json
```

该命令会编译当前 App Spec，并与 `.appstruct/schema.snapshot.json` 比较，然后报告稳定的问题码：

| 代码 | 含义 |
| --- | --- |
| `AS4201` | 破坏性 Schema 或数据变更 |
| `AS4202` | 现有表上的操作可能锁行 |
| `AS4203` | 需要人工审查 SQL 的操作 |
| `AS4204` | 未提供默认值或回填的非空列新增 |

错误会以非零退出码结束。提供 `--deny-warnings` 时，警告也会升级为错误。

## 下一步

- [升级](upgrading.zh-CN.md)：升级计划含破坏性变更时
- [部署](deployment.zh-CN.md)：生产环境的 `migrate status` / `migrate apply`
- [Schema 索引](schema-indexes.zh-CN.md)：添加索引可能触发锁表警告
