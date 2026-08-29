import { lazy, useState, type ComponentType } from "react";
import { resources } from "../generated/resources";
import { customPages } from "../generated/registry";
import type { AppStructRegistry, PageComponentProps } from "../generated/registry";
import { Navigate, Outlet, RuntimeRouter, createRuntimeRouter, type RuntimeRoute } from "../navigation";
import { ResourceActorProvider, useVisibleResources } from "../resource";
import { Layout } from "./Layout";

const ResourceDetail = lazy(() => import("../pages/ResourceDetail").then(({ ResourceDetail: component }) => ({ default: component })));
const ResourceForm = lazy(() => import("../pages/ResourceForm").then(({ ResourceForm: component }) => ({ default: component })));
const ResourceList = lazy(() => import("../pages/ResourceList").then(({ ResourceList: component }) => ({ default: component })));

export function App({ registry }: { registry?: AppStructRegistry }) {
  const [router] = useState(() => {
    const pageComponents = registry?.pages as Record<string, ComponentType<PageComponentProps>> | undefined;
    const routes: RuntimeRoute[] = [
      {
        id: "_layout",
        component: () => <Layout resources={resources} pages={customPages} />,
        children: [
          { path: "/", component: HomeRedirect },
          ...resourceRoutes(registry),
          ...customPages.map((page) => ({
            path: absolutePath(page.path),
            component: pageComponents?.[String(page.component)] ?? PageRendererUnavailable,
          })),
          { path: "/empty", component: EmptyPage },
        ],
      },
    ];
    return createRuntimeRouter(PublicRoot, routes);
  });
  return <RuntimeRouter router={router} />;
}

function PublicRoot() {
  return <ResourceActorProvider user={null}><Outlet /></ResourceActorProvider>;
}

function resourceRoutes(registry?: AppStructRegistry): RuntimeRoute[] {
  return resources.flatMap((resource) => [
    { path: `/${resource.slug}`, component: () => <ResourceList resource={resource} resources={resources} /> },
    { path: `/${resource.slug}/new`, component: () => <ResourceForm resource={resource} resources={resources} registry={registry} /> },
    { path: `/${resource.slug}/$id`, component: () => <ResourceDetail resource={resource} /> },
    { path: `/${resource.slug}/$id/edit`, component: () => <ResourceForm resource={resource} resources={resources} registry={registry} /> },
  ]);
}

function HomeRedirect() {
  const first = useVisibleResources(resources)[0];
  return <Navigate to={`/${first?.slug ?? customPages[0]?.path ?? "empty"}`} replace />;
}

function EmptyPage() {
  return <main className="page"><h1>No resources</h1></main>;
}

function PageRendererUnavailable() {
  return <main className="page"><div className="alert" role="alert">Page renderer unavailable</div></main>;
}

function absolutePath(path: string): string {
  return `/${path.replace(/^\/+/, "")}`;
}
