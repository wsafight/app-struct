import type { ComponentType } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { Layout } from "./Layout";
import { resources } from "../generated/resources";
import { ResourceForm } from "../pages/ResourceForm";
import { ResourceList } from "../pages/ResourceList";
import { ResourceDetail } from "../pages/ResourceDetail";
import { customPages } from "../generated/registry";
import type { AppStructRegistry, PageComponentProps } from "../generated/registry";
import { ResourceActorProvider, useVisibleResources } from "../resource";

export function App({ registry }: { registry?: AppStructRegistry }) {
  const pageComponents = registry?.pages as Record<string, ComponentType<PageComponentProps>> | undefined;
  return (
    <ResourceActorProvider user={null}><Routes>
      <Route element={<Layout resources={resources} pages={customPages} />}>
        <Route index element={<HomeRedirect />} />
        {resources.map((resource) => (
          <Route key={resource.name}>
            <Route path={resource.slug} element={<ResourceList resource={resource} resources={resources} />} />
            <Route path={`${resource.slug}/new`} element={<ResourceForm resource={resource} resources={resources} registry={registry} />} />
            <Route path={`${resource.slug}/:id`} element={<ResourceDetail resource={resource} />} />
            <Route path={`${resource.slug}/:id/edit`} element={<ResourceForm resource={resource} resources={resources} registry={registry} />} />
          </Route>
        ))}
        {customPages.map((page) => {
          const Component = pageComponents?.[String(page.component)];
          return <Route key={page.name} path={page.path} element={Component ? <Component /> : <main className="page"><div className="alert" role="alert">Page renderer unavailable</div></main>} />;
        })}
        <Route path="empty" element={<main className="page"><h1>No resources</h1></main>} />
      </Route>
    </Routes></ResourceActorProvider>
  );
}

function HomeRedirect() {
  const first = useVisibleResources(resources)[0];
  return <Navigate to={`/${first?.slug ?? customPages[0]?.path ?? "empty"}`} replace />;
}
