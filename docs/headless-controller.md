# Headless Web Controllers

Generated list and detail pages use the same headless controller hooks available to custom page
components. The hooks keep TanStack Query keys, list/read permission gates, loading/error state,
request cancellation, refetching, and post-mutation invalidation consistent with generated CRUD.

Custom pages receive the generated resources through `PageComponentProps`:

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

Use `useResourceDetailController(resource, id)` for record/read permission, cached detail loading,
update visibility, error state, and refetch. `useResourceListController.runChange` coordinates an
application-owned async mutation and invalidates every query for that resource after success.
Call the typed methods on `resource.api` inside that operation; the generated backend remains the
authorization and validation authority.

The current controller slice does not own URL parameter parsing or TanStack Form state. Generated
list pages still map router search parameters into `ListQuery`, and generated forms still own Zod,
field errors, ETag conflicts, and unsaved-change protection. Those contracts should move behind
controller APIs before custom forms can fully replace generated forms without duplicating behavior.
