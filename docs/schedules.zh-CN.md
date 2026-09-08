# 定时任务

Jobs 模块支持经过时间的间隔，以及五字段日历 Cron 表达式。日历表达式按 UTC 求值。编译器会在生成前校验完整表达式。

- `@every Ns`、`@every Nm` 和 `@every Nh` 从最近一次调度器领取开始运行。间隔必须在一秒到 24 小时之间。
- 五字段 Cron 使用 `minute hour day-of-month month day-of-week`，包含生成运行时 Cron 解析器接受的列表、范围和步进。

```yaml
modules:
  jobs:
    enabled: true
    schedules:
      cleanup:
        cron: "@every 15m"
        queue: default
        kind: maintenance.cleanup
        payload: '{"scope":"expired"}'
      weekday-digest:
        cron: "0 9 * * 1-5"
        queue: default
        kind: reports.weekday-digest
```

启动时，每个后端会把声明的调度集合对账到 `_appstruct_job_schedules`。当前定义会被插入或重新启用；更改后的间隔定义从当前时间重新开始，更改后的日历定义前进到下一次 UTC 出现时间，App Spec 中不再存在的行会被禁用。首次启用 Jobs 的发布，请先部署匹配的迁移再启动。

调度使用只触发一次、跳过积压的语义。如果进程在多个周期内不可用，下一次 worker 迭代只会入队一个 Job，并根据当前数据库时间计算下次运行。它不会重放每一个错过的间隔。推进 `next_run_at` 和插入 Job 共享一个 PostgreSQL 事务，生成的幂等键会防止已领取出现时间被重复插入。日历调度始终前进到第一个未来匹配时间。

多个 API 副本可以对同一个 PostgreSQL 数据库运行调度器。带 `FOR UPDATE SKIP LOCKED` 的行锁确保只有一个副本领取到期定义。通过 `/admin/jobs` 监控排队/死信 Job。管理员可以在 `/admin/schedules` 检查定义、暂停或恢复活动调度，并立即入队一次运行。暂停状态与定义对账分开存储，因此进程重启不会静默恢复调度。定义仍由 App Spec 拥有，不能从 Admin UI 编辑。
