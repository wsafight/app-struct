# 无头 Web 控制器

生成的列表和详情页使用与自定义页面组件相同的无头控制器 hooks。这些 hooks 使 TanStack Query 键、列表/读取权限门、加载/错误状态、请求取消、重新获取以及变更后失效与生成的 CRUD 保持一致。

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
