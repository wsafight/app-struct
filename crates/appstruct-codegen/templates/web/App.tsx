import { t } from "../ui";
import { useState } from "react";
import { resources } from "../generated/resources";
import { workspacePath, type AppStructRegistry } from "../generated/registry";
import {
  Navigate,
  Outlet,
  RuntimeRouter,
  createRuntimeRouter,
  type RuntimeRoute,
} from "../navigation";
import { ResourceActorProvider, useVisibleResources } from "../resource";
import { Layout } from "./Layout";
import { resourceRoutes } from "./ResourceRoutes";
import {
  customPageRoutes,
  PageScopeLayout,
  workspacePages,
} from "./PageRoutes";

export function App({ registry }: { registry?: AppStructRegistry }) {
  const [router] = useState(() => {
    const routes: RuntimeRoute[] = [
      {
        id: "_public",
        component: () => <PageScopeLayout registry={registry} scope="public" />,
        children: customPageRoutes(registry, "public"),
      },
      {
        id: "_layout",
        component: () =>
          registry?.layouts?.workspace ? (
            <PageScopeLayout registry={registry} scope="workspace" />
          ) : (
            <Layout resources={resources} pages={workspacePages} />
          ),
        children: [
          { path: workspacePath, component: HomeRedirect },
          ...resourceRoutes(registry),
          ...customPageRoutes(registry, "workspace"),
          { path: "/empty", component: EmptyPage },
        ],
      },
    ];
    return createRuntimeRouter(PublicRoot, routes);
  });
  return <RuntimeRouter router={router} />;
}

function PublicRoot() {
  return (
    <ResourceActorProvider user={null}>
      <Outlet />
    </ResourceActorProvider>
  );
}

function HomeRedirect() {
  const first = useVisibleResources(resources)[0];
  return (
    <Navigate
      to={`/${first?.slug ?? workspacePages[0]?.path ?? "empty"}`}
      replace
    />
  );
}

function EmptyPage() {
  return (
    <main className="page">
      <h1>{t("No resources")}</h1>
    </main>
  );
}
