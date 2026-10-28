# 通用表单规则

字段在标量上下限之外支持同记录规则。编译器检查引用和类型；生成 Web 复用 `valueError` 与 `recordRuleErrors`，不引入直接 Zod 依赖。

```yaml
fields:
  starts: { type: datetime, required: true }
  ends: { type: datetime, greater_than: starts }
  needs_note: { type: boolean, required: true }
  note:
    type: text
    required_when: { field: needs_note, equals: true }
```

`greater_than` 为严格大于，相等也失败。两字段必须是同一种 integer、bigint、decimal、date 或 datetime 类型，且都有值时才比较；精确小数与微秒级日期时间保留精度。`required_when` 的条件字段为同实体 boolean、text 或 enum，`equals` 必须为匹配的布尔或字符串值；目标为可空标量。条件命中时，缺失、null、空文本判为未填写，0 和 false 视为有值。条件取消不会自动清空已有目标值。

规则目标必须为普通字段，依赖必须为其他同实体字段。两者要求不限制字段读写访问，避免通过校验错误泄露隐藏状态；条件可引用 Workflow 状态。涉及受限字段、其他记录或复杂业务逻辑时使用 Policy/Hook，不接受任意表达式或 SQL。

后端在输入 Hook 与权限检查后、写入前校验完整候选记录。创建、部分更新、CSV、批量更新、聚合明细和 Workflow 转换执行同一检查，省略另一字段不能绕过规则。普通无效写入返回 422 和字段错误；批量/CSV 按行报告。事务回滚使被拒绝记录相关的 Hook 副作用不会提交。原始 SQL、seed 和应用直接写实体 Model 仍由应用负责；本规则不生成 SQL CHECK 约束。

Web 校验已知的可编辑值和未改动基线，服务端默认值及运行时状态以服务端为准；批量和行内控件可能不持有完整记录，需展示服务端字段错误。语言配置只改变文案和显示，不改变规则语义。须从 Spec 重新编译当前 IR 18，旧 IR 会被拒绝。
