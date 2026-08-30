import { useState, type ComponentType } from "react";
import { resources } from "../generated/resources";
import { customPages } from "../generated/registry";
import type { AppStructRegistry, PageComponentProps } from "../generated/registry";
import { Navigate, Outlet, RuntimeRouter, createRuntimeRouter, type RuntimeRoute } from "../navigation";
import { ResourceActorProvider, useVisibleResources } from "../resource";
import { Layout } from "./Layout";
import { resourceRoutes } from "./ResourceRoutes";

export function App({ registry }: { registry?: AppStructRegistry }) {
  const [router] = useState(() => {
    const routes: RuntimeRoute[] = [
      {
        id: "_layout",
        component: () => <Layout resources={resources} pages={customPages} />,
        children: [
          { path: "/", component: HomeRedirect },
          ...resourceRoutes(registry),
          ...customPageRoutes(registry),
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

function customPageRoutes(registry?: AppStructRegistry): RuntimeRoute[] {
  const pageComponents = registry?.pages as Record<string, ComponentType<PageComponentProps>> | undefined;
  return customPages.map((page) => {
    const Component = pageComponents?.[String(page.component)];
    return {
      path: absolutePath(page.path),
      component: Component ? () => <Component resources={resources} /> : PageRendererUnavailable,
    };
  });
}

function absolutePath(path: string): string {
  return `/${path.replace(/^\/+/, "")}`;
}
