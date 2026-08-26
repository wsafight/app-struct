import { Navigate, Route, Routes } from "react-router-dom";
import { Layout } from "./Layout";
import { resources } from "../generated/resources";
import { ResourceForm } from "../pages/ResourceForm";
import { ResourceList } from "../pages/ResourceList";
import { ResourceDetail } from "../pages/ResourceDetail";
import { customPages } from "../generated/registry";
import type { AppStructRegistry } from "../generated/registry";

export function App({ registry }: { registry?: AppStructRegistry }) {
  const first = resources[0];
  const firstPath = first?.slug ?? customPages[0]?.path ?? "empty";
  return (
    <Routes>
      <Route element={<Layout resources={resources} pages={customPages} />}>
        <Route index element={<Navigate to={`/${firstPath}`} replace />} />
        {resources.map((resource) => (
          <Route key={resource.name}>
            <Route path={resource.slug} element={<ResourceList resource={resource} resources={resources} />} />
            <Route path={`${resource.slug}/new`} element={<ResourceForm resource={resource} resources={resources} registry={registry} />} />
            <Route path={`${resource.slug}/:id`} element={<ResourceDetail resource={resource} />} />
            <Route path={`${resource.slug}/:id/edit`} element={<ResourceForm resource={resource} resources={resources} registry={registry} />} />
          </Route>
        ))}
        {customPages.map((page) => {
          const Component = registry?.pages[page.component];
          return <Route key={page.name} path={page.path} element={Component ? <Component /> : <main className="page"><div className="alert" role="alert">Page renderer unavailable</div></main>} />;
        })}
        <Route path="empty" element={<main className="page"><h1>No resources</h1></main>} />
      </Route>
    </Routes>
  );
}
