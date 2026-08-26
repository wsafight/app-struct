import { Navigate, Route, Routes } from "react-router-dom";
import { Layout } from "./Layout";
import { resources } from "../generated/resources";
import { ResourceForm } from "../pages/ResourceForm";
import { ResourceList } from "../pages/ResourceList";
import { ResourceDetail } from "../pages/ResourceDetail";

export function App() {
  const first = resources[0];
  return (
    <Routes>
      <Route element={<Layout resources={resources} />}>
        <Route index element={<Navigate to={first ? `/${first.slug}` : "/empty"} replace />} />
        {resources.map((resource) => (
          <Route key={resource.name}>
            <Route path={resource.slug} element={<ResourceList resource={resource} resources={resources} />} />
            <Route path={`${resource.slug}/new`} element={<ResourceForm resource={resource} resources={resources} />} />
            <Route path={`${resource.slug}/:id`} element={<ResourceDetail resource={resource} />} />
            <Route path={`${resource.slug}/:id/edit`} element={<ResourceForm resource={resource} resources={resources} />} />
          </Route>
        ))}
        <Route path="empty" element={<main className="page"><h1>No resources</h1></main>} />
      </Route>
    </Routes>
  );
}
