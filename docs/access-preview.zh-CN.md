# 权限解释与预览

`appstruct access preview` 编译当前项目并输出只读权限矩阵，不连接数据库，也不执行应用自定义 Rust Policy。

```bash
appstruct access preview
appstruct access preview --authenticated --role admin
appstruct --format json access preview --entity Project --actor-id 01990000-0000-7000-8000-000000000001 --record sample-project.json
```

`--role` 可重复，为同一示例操作者指定已声明的角色。`--record` 是本地 API 记录 JSON 对象，最大 1 MiB，需同时指定 `--entity`。输出包括资源 CRUD、字段读取及普通更新的规则、判断和原因。字段项合并资源权限与字段权限；生成字段、主键及 Workflow 字段不能通过普通更新写入；聚合子项须通过父资源明细 API 写入。owner 规则需要示例操作者 ID 和所属关系，缺少上下文返回 `requires_record`；租户资源需要 `--tenant-id`，示例记录属于另一租户时拒绝。JSON 输出不复制示例字段值。

`scope: declarations_only` 与 `runtime_policy: not_evaluated` 明确限定结论。`allowed` 不代表当前成员身份、数据库状态和应用 Policy 一定放行；这些仍须通过应用测试验证，参见[业务扩展示例](business-extensions.zh-CN.md)。本命令不修改角色或授予权限。
