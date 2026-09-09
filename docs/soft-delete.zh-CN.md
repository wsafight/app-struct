# 软删除与历史

当删除应当归档而不是去掉行时，设置 `soft_delete: true`。实体需要可空的 `datetime` 字段 `deleted_at`。审计事件仍是历史来源。

```yaml
soft_delete: true
fields:
  deleted_at:
    type: datetime
```

常规 list、detail、聚合和关系查询会排除已进入回收站的行。删除会保留该行，设置 `deleted_at`，推进修订号，并记录现有审计事件。生成资源会增加 `/_trash` 以提供已授权的回收站视图，以及 `/_restore` 以进行带修订检查的逐条恢复。React 列表会暴露回收站开关和恢复操作。审计事件继续强制执行 actor 与租户访问。

## 下一步

- [记录动态](activity.zh-CN.md)：在记录旁展示经授权的历史
- [保存视图](saved-views.zh-CN.md)：列表状态可以包含回收站模式
- [关系展示](relation-display.zh-CN.md)：lookup 遵守回收站状态
