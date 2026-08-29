import { lazy } from "react";
import { resources } from "../generated/resources";
import type { AppStructRegistry } from "../generated/registry";
import { validateResourceSearch, type RuntimeRoute } from "../navigation";

const ResourceDetail = lazy(() =>
  import("../pages/ResourceDetail").then(({ ResourceDetail: component }) => ({ default: component })),
);
const ResourceForm = lazy(() =>
  import("../pages/ResourceForm").then(({ ResourceForm: component }) => ({ default: component })),
);
const ResourceList = lazy(() =>
  import("../pages/ResourceList").then(({ ResourceList: component }) => ({ default: component })),
);

export function resourceRoutes(registry?: AppStructRegistry): RuntimeRoute[] {
  return resources.flatMap((resource) => [
    {
      path: `/${resource.slug}`,
      component: () => <ResourceList resource={resource} resources={resources} />,
      validateSearch: validateResourceSearch,
    },
    {
      path: `/${resource.slug}/new`,
      component: () => <ResourceForm resource={resource} resources={resources} registry={registry} />,
    },
    { path: `/${resource.slug}/$id`, component: () => <ResourceDetail resource={resource} /> },
    {
      path: `/${resource.slug}/$id/edit`,
      component: () => <ResourceForm resource={resource} resources={resources} registry={registry} />,
    },
  ]);
}
