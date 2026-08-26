import type { ComponentType } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { AuthProvider, RequireAuth } from "../auth/Auth";
import { ForgotPasswordPage, LoginPage, RegisterPage, ResetPasswordPage } from "../auth/AuthPages";
import { resources } from "../generated/resources";
import { customPages } from "../generated/registry";
import type { AppStructRegistry, PageComponentProps } from "../generated/registry";
import { ResourceDetail } from "../pages/ResourceDetail";
import { ResourceForm } from "../pages/ResourceForm";
import { ResourceList } from "../pages/ResourceList";
import { Layout } from "./Layout";

export function App({ registry }: { registry?: AppStructRegistry }) {
  const first = resources[0];
  const firstPath = first?.slug ?? customPages[0]?.path ?? "empty";
  const pageComponents = registry?.pages as Record<string, ComponentType<PageComponentProps>> | undefined;
  return <AuthProvider><Routes>
    <Route path="login" element={<LoginPage />} />
    <Route path="register" element={<RegisterPage />} />
    <Route path="forgot-password" element={<ForgotPasswordPage />} />
    <Route path="reset-password" element={<ResetPasswordPage />} />
    <Route element={<RequireAuth />}>
      <Route element={<Layout resources={resources} pages={customPages} />}>
        <Route index element={<Navigate to={`/${firstPath}`} replace />} />
        {resources.map((resource) => <Route key={resource.name}>
          <Route path={resource.slug} element={<ResourceList resource={resource} resources={resources} />} />
          <Route path={`${resource.slug}/new`} element={<ResourceForm resource={resource} resources={resources} registry={registry} />} />
          <Route path={`${resource.slug}/:id`} element={<ResourceDetail resource={resource} />} />
          <Route path={`${resource.slug}/:id/edit`} element={<ResourceForm resource={resource} resources={resources} registry={registry} />} />
        </Route>)}
        {customPages.map((page) => {
          const Component = pageComponents?.[String(page.component)];
          return <Route key={page.name} path={page.path} element={Component ? <Component /> : <main className="page"><div className="alert" role="alert">Page renderer unavailable</div></main>} />;
        })}
        <Route path="empty" element={<main className="page"><h1>No resources</h1></main>} />
      </Route>
    </Route>
  </Routes></AuthProvider>;
}
