# 生成的资源查询

生成的资源集合端点支持用于表格式导航的偏移分页，以及用于稳定遍历大型结果集的游标分页。搜索和已声明的过滤器在两种模式下都可用。本页给出请求、响应、OpenAPI 和 TypeScript 客户端契约。

## 偏移分页

偏移模式仍是默认，并包含精确总数：

```text
GET /api/tasks/?page=2&page_size=25&sort=-created_at&filter[status]=todo
```

```json
{
  "data": [],
  "meta": { "page": 2, "page_size": 25, "total": 0 }
}
```

`page` 从 1 开始，必须介于 1 和 10,000 之间。`page_size` 默认为 25，必须介于 1 和 100 之间。搜索词按字面子串匹配；在应用 SQL `LIKE` 模式之前会转义 `%`、`_` 和 `\`。已声明的排序按顺序应用，并在需要时追加主键以保持排序确定性。

## 游标分页

提供 `limit` 或 `cursor` 会选择游标模式。用 `limit` 开始遍历；传入返回的不透明 `next_cursor` 以继续：

```text
GET /api/tasks/?limit=25&filter[status]=todo
GET /api/tasks/?limit=25&cursor=<next_cursor>&filter[status]=todo
```

```json
{
  "data": [],
  "meta": { "limit": 25, "next_cursor": null, "has_more": false }
}
```

游标模式按资源主键升序排序，最多获取 100 条记录，并且不运行总数查询。`page`、`page_size` 和 `sort` 不能与游标模式组合。游标令牌是带版本的 Base64URL 值，属于 API 实现细节；客户端必须原样保留。更改搜索或过滤参数后，从第一页重新开始。

生成的 TypeScript 客户端分别暴露这两种模式：

```ts
const page = await taskApi.list({ page: 1, page_size: 25 });
const first = await taskApi.listCursor({ limit: 25, filters: { status: "todo" } });
const next = await taskApi.listCursor({
  limit: 25,
  cursor: first.meta.next_cursor ?? undefined,
  filters: { status: "todo" },
});
```

## 关系过滤器

当关系字段和目标字段都声明 `filterable: true` 时，出站关系可以暴露一跳目标过滤器。例如，可过滤的 `Task.project` 关系和可过滤的 `Project.status` 字段会产出：

```text
GET /api/tasks/?filter[project.status]=active
GET /api/tasks/?filter[project.created_at][gte]=2026-01-01T00:00:00Z
```

只接受 OpenAPI 中发布的生成过滤路径。不支持的参数会以 invalid-query 响应失败。关系过滤器实现为目标键子查询。目标实体的列表访问规则和租户范围会在其值过滤器之前应用于每个子查询内部，因此结果行和偏移总数无法泄露不可访问的相关记录。

列表和详情载荷当前将关系字段暴露为外键值。它们不会为每条记录发起目标查找，因此不会创建隐式 N+1 查询路径。未来的关系展开契约必须批量处理目标键，保留目标读策略和租户范围，并在 Admin 表格能显示相关标签而不是标识符之前，在 OpenAPI 和生成的 TypeScript 客户端中发布展开后的响应形状。

## 聚合与分组

每个生成的资源都暴露一个有界报表端点：

```text
GET /api/tasks/_aggregate?metrics=count,sum:priority,avg:priority&group_by=project.status&limit=20
```

`metrics` 是逗号分隔列表。`count`（或 `count:*`）始终可用。标记为 `filterable: true` 的字段在为整数、bigint 或 decimal 值时可以使用 `sum` 和 `avg`；`min` 和 `max` 额外支持 string、enum、date 和 datetime 字段。`group_by` 接受除 JSON 以外的可过滤标量字段，以及 `project.status` 这类一跳 to-one 关联维度；每个请求最多使用一个关联维度。重复或不支持的指标和分组会作为无效查询失败。

```json
{
  "data": [
    {
      "group_project_status": "active",
      "count": 12,
      "sum_priority": 31,
      "avg_priority": 2.5833333333333335
    }
  ],
  "meta": {
    "metrics": ["count", "sum:priority", "avg:priority"],
    "group_by": ["project.status"],
    "limit": 20,
    "order": "desc"
  }
}
```

结果属性使用 `group_<field>` 和 `<metric>_<field>` 别名，关联维度中的点会转换为下划线。省略 `metrics` 参数时默认为 `count`。`limit` 默认为 100，必须介于 1 和 500 之间；它限制返回的分组数，而不是源行数。分组先按第一个指标排序（默认 `desc`，也可使用 `order=asc`），再按维度排序，因此 Top-N 结果是确定的。搜索、标量过滤器和一跳关系过滤器使用与列表查询相同的参数。源实体和分组目标实体都会应用列表权限、租户、软删除和字段读取规则，因此聚合不会泄露不可访问的数据。

生成的 TypeScript 客户端接受数组并将它们序列化为逗号分隔的传输格式：

```ts
const report = await taskApi.aggregate({
  metrics: ["count", "sum:priority"],
  group_by: ["project.status"],
  limit: 20,
  filters: { "project.status": "active" },
});
```

### 声明式资源图表

实体可以为生成的资源摘要声明经过校验的图表：

```yaml
entities:
  Task:
    charts:
      total_tasks:
        label: Total tasks
        type: kpi
        measure: count
      by_project_status:
        label: Tasks by project status
        type: horizontal_bar
        dimension: project.status
        measure: count
        limit: 20
```

当前支持 `kpi`、`bar`、`horizontal_bar` 和 `donut`。KPI 不使用维度；其他类型必须指定可过滤的标量字段或一跳 to-one 关联维度。指标沿用聚合端点的 `count`、`sum:<field>`、`avg:<field>`、`min:<field>` 和 `max:<field>` 契约。编译器会拒绝未知路径、更深层关联、不兼容指标、1-100 之外的上限，以及单个实体超过 20 个图表的配置。日期分桶和折线图会等聚合端点具备明确的时间分桶契约后再支持。

## 字段级访问

字段可以声明 `access.read` 和 `access.write` 规则。缺失的字段规则继承实体的操作策略。读规则在行级范围之后、JSON 序列化之前强制执行，因此不可见的字段会从列表、详情和写入响应中省略。写规则在 hooks 运行之后、校验/持久化之前检查；提交没有权限的字段会返回与对应实体操作相同的授权错误。生成的 Web manifest 使用这些规则隐藏未授权的列和表单控件，而后端仍保持权威。

```yaml
api_key:
  type: string
  required: false
  access:
    read: { role: admin }
    write: { role: admin }
```

## 下一步

- [标量值](scalar-values.zh-CN.md)：bigint、decimal 与日期时间的 JSON 表示
- [保存视图](saved-views.zh-CN.md)：持久化列表查询状态
- [关系展示](relation-display.zh-CN.md)：批量 lookup 标签
