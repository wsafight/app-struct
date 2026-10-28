# 无头 Web 控制器

生成的列表和详情页使用与自定义页面组件相同的无头控制器 hooks。这些 hooks 使 TanStack Query 键、列表/读取权限门、加载/错误状态、请求取消、重新获取以及变更后失效与生成的 CRUD 保持一致。启用 Tenant 时，资源查询键自动包含当前组织作用域。

自定义页面通过 `PageComponentProps` 接收生成的资源：

```tsx
import { useResourceListController } from "../../generated/web/src/controller";
import type { PageComponentProps } from "../../generated/web/src/generated/registry";
import type { ResourceDefinition } from "../../generated/web/src/resource";

export function ProjectDashboard({ resources }: PageComponentProps) {
  const project = resources.find((resource) => resource.name === "Project");
  if (!project) return <main className="page">Project resource unavailable</main>;
  return <ProjectData resource={project} />;
}

function ProjectData({ resource }: { resource: ResourceDefinition }) {
  const controller = useResourceListController(resource, {
    cacheKey: "dashboard:recent",
    query: { page: 1, page_size: 10, sort: "-created_at" },
  });
  if (!controller.canList) return <main className="page">Access denied</main>;
  if (controller.pending) return <main className="page">Loading...</main>;
  return <main className="page">{controller.records.length} recent projects</main>;
}
```

使用 `useResourceDetailController(resource, id)` 处理记录/读取权限、缓存的详情加载、更新可见性、错误状态和重新获取。`useResourceListController.runChange` 协调应用自有的异步变更，并在成功后使该资源的每个查询失效。在该操作内调用 `resource.api` 上的类型化方法；生成的后端仍是授权和校验权威。

`useResourceUrlController(resource)` 拥有规范化的分页、搜索、排序、已授权过滤器、回收站状态和 URL 更新。把它的 `query` 和 `trashMode` 传入列表控制器。即使自定义页面复用调用方提供的 `cacheKey`，查询键也包含实际查询对象。

`useResourceFormController(resource, options)` 拥有 TanStack Form、精确标量校验、字段错误、修订冲突、变更失效和脏状态。在加载编辑记录后挂载它，并用资源和 ID 作为编辑器的 key。选项接受 `id`、`initialRecord`、可选的 `refetchRecord` 以及 `onSaved`。用 `controller.form.Field` 渲染字段；订阅 `form.state.isDirty` 和 `isSubmitting` 以连接现有的 `useUnsavedChanges` 导航守卫。生成的表单消费同一控制器。

冲突会保留草稿。`reloadRecord()` 显式用最新记录和修订替换它，清除字段错误并重置脏状态。成功保存也会在调用 `onSaved` 之前重置脏状态。自定义页面拥有自己的展示和目标路由。

## 共享字段校验

生成 Web 不再直接依赖 Zod。`field-values.ts` 的 `valueError(raw, field)` 是表单、行内编辑、筛选、批量操作和明细编辑共用的字段规则；TanStack Form 提交校验逐字段调用它，以 `{ fields: { 字段名: 错误消息 } }` 返回错误，无错误时返回 `undefined`。自定义表单应复用该规则，避免另写一份 schema。

字段校验保留 UUID 的版本/variant 及 nil/max 值、必填、精确数值、JSON 和日期语义。金额/币种配对属于跨字段约束，仍由表单控制器单独检查，包括零金额与可选空值。`toApiValue` 在提交转换前再次校验，并保留 bigint/decimal 精度。前端校验用于及时反馈，后端仍负责权威校验和授权。开发工具的传递依赖可以继续包含 Zod。

## 编辑冲突对照

更新的 `If-Match` 固定为表单基线修订号，其他组件刷新详情缓存不会悄悄改变它。`compareConflict()` 拉取最新记录并保留草稿；`comparison.fields` 展示编辑前、服务器最新值和我的修改。单方变化自动选择，双方不同修改需调用 `chooseConflict(name, "mine" | "latest")` 明确选择。`applyConflict()` 以对照时的最新版本为新基线并应用所选草稿，不自动提交；检查后再次保存。期间再次发生并发编辑仍会返回冲突。自定义页面仍可使用会丢弃草稿的 `reloadRecord()`。

[通用表单规则](form-validation.zh-CN.md)复用 `valueError` 与 `recordRuleErrors`；服务端对 Hook 处理后的完整候选记录校验，包括部分更新。
