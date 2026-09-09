# 标量值

JSON、OpenAPI 和 TypeScript 把大数字保持为十进制字符串，这样在生成栈中不会丢精度。PostgreSQL 列和 Rust 类型仍是原生整数。

## Bigint 与 decimal

业务 `bigint` 字段在 JSON、OpenAPI 和 TypeScript 中使用十进制字符串。PostgreSQL 列和 Rust 值仍是有符号 64 位整数。这涵盖实体字段、值对象、Workflow 输入以及 bigint 聚合/分组值。Decimal 值与聚合同样使用字符串。框架修订计数器和分页元数据保留现有数字契约。

生成后端只在 JavaScript 安全整数范围内接受遗留 JSON 整数输入，即 `-9007199254740991` 到 `9007199254740991`。更大的值必须是字符串。响应始终使用字符串。采用此预览契约时，请同时重新生成并部署对应的客户端；对 bigint 字段做算术的客户端必须显式使用 BigInt 或十进制库。现有数据和迁移文件不会改变。

共享的 Web 字段值 API 会在表单、行内编辑和 Workflow 对话框中保留这些字符串。数值边界使用精确的十进制比较。货币格式化从不把金额转换成 JavaScript Number。JSON 字段仍是任意 JSON，不会为嵌套值推断类型。

## 日期时间

日期时间 API 值标识 UTC 时刻。生成控件显示浏览器本地日历时间，包含秒以及最多 PostgreSQL 的六位小数，并把编辑后的值转回 UTC。未改动的值会保留原始时刻，包括夏令时重复小时中较晚的那一次。新输入的歧义本地时间使用浏览器较早的那一次；无效日历日期以及落在夏令时空隙中的时间会被拒绝。纯日期字段不做时区转换。

## 下一步

- [资源查询](data-querying.zh-CN.md)：筛选、聚合与分组
- [业务 UI 语义](business-ui-semantics.zh-CN.md)：金额展示
