# Headless Web Controllers

Generated list and detail pages use the same headless controller hooks available to custom page
components. The hooks keep TanStack Query keys, list/read permission gates, loading/error state,
request cancellation, refetching, and post-mutation invalidation consistent with generated CRUD.
When Tenant is enabled, resource query keys automatically include the current organization scope.

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

`useResourceUrlController(resource)` owns normalized pagination, search, sort, authorized filters,
trash state and URL updates. Pass its `query` and `trashMode` into the list controller. Query keys
include the actual query object even when a custom page reuses its caller-supplied `cacheKey`.

`useResourceFormController(resource, options)` owns TanStack Form, exact scalar validation, field
errors, revision conflicts, mutation invalidation and dirty state. Mount it after loading an edit
record and key the editor by resource and ID. Options accept `id`, `initialRecord`, an optional
`refetchRecord`, and `onSaved`. Render fields with `controller.form.Field`; subscribe to
`form.state.isDirty` and `isSubmitting` to connect the existing `useUnsavedChanges` navigation guard.
The generated form consumes this same controller.

Conflicts preserve the draft. `reloadRecord()` explicitly replaces it with the latest record and
revision, clears field errors and resets dirty state. A successful save also resets dirty state
before invoking `onSaved`. Custom pages own their presentation and destination route.

## Shared field validation

Generated Web no longer directly depends on Zod. `valueError(raw, field)` in `field-values.ts`
provides the shared field rules for forms, inline editing, filters, bulk operations and detail
editing. TanStack Form calls it per field on submit, returning `{ fields: { fieldName: message } }`
for errors or `undefined` on success. Custom forms should reuse these rules.

Validation retains UUID version/variant and nil/max values, required fields, exact numbers, JSON
and date semantics. Amount/currency pairing remains a separate cross-field constraint, including
zero amounts and optional empty values. `toApiValue` validates again before conversion and retains
bigint/decimal precision. Frontend validation provides feedback; the backend remains authoritative
for validation and authorization. Tooling may still depend on Zod transitively.

## Compare editing conflicts

Updates pin `If-Match` to the form baseline revision, even if another consumer refreshes the detail cache. `compareConflict()` fetches the latest record without losing the draft. `comparison.fields` exposes original, latest and mine values; unchanged fields resolve automatically and competing changes require `chooseConflict(name, "mine" | "latest")`. `applyConflict()` makes the compared version the new baseline and applies the chosen draft. It does not submit: review and save explicitly. A second concurrent edit returns another conflict. `reloadRecord()` remains an explicit draft-discarding alternative for custom pages.

[Common form rules](form-validation.md) reuse `valueError` plus `recordRuleErrors`; the server validates the final candidate after Hooks, including partial edits.
