import type { ComponentType } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { AuditPage } from "../audit/AuditPage";
import { AuthProvider, RequireAuth } from "../auth/Auth";
import { ForgotPasswordPage, LoginPage, RegisterPage, ResetPasswordPage } from "../auth/AuthPages";
import { auditAccess, resources } from "../generated/resources";
import { customPages } from "../generated/registry";
import type { AppStructRegistry, PageComponentProps } from "../generated/registry";
import { ResourceDetail } from "../pages/ResourceDetail";
import { ResourceForm } from "../pages/ResourceForm";
import { ResourceList } from "../pages/ResourceList";
import { useCanAccessRule, useVisibleResources } from "../resource";
import { InvitationAcceptPage, OrganizationPage, RequireTenant, TenantProvider } from "../tenant/Tenant";
import { Layout } from "./Layout";

export function App({ registry }: { registry?: AppStructRegistry }) {
  const pageComponents = registry?.pages as Record<string, ComponentType<PageComponentProps>> | undefined;
  return <AuthProvider><Routes>
    <Route path="login" element={<LoginPage />} /><Route path="register" element={<RegisterPage />} /><Route path="forgot-password" element={<ForgotPasswordPage />} /><Route path="reset-password" element={<ResetPasswordPage />} /><Route path="accept-invitation" element={<InvitationAcceptPage />} />
    <Route element={<RequireAuth />}><Route element={<TenantProvider><RequireTenant /></TenantProvider>}><Route element={<Layout resources={resources} pages={customPages} />}>
      <Route index element={<HomeRedirect />} />
      {resources.map((resource) => <Route key={resource.name}><Route path={resource.slug} element={<ResourceList resource={resource} resources={resources} />} /><Route path={`${resource.slug}/new`} element={<ResourceForm resource={resource} resources={resources} registry={registry} />} /><Route path={`${resource.slug}/:id`} element={<ResourceDetail resource={resource} />} /><Route path={`${resource.slug}/:id/edit`} element={<ResourceForm resource={resource} resources={resources} registry={registry} />} /></Route>)}
      <Route path="audit" element={<AuditPage />} />
      {customPages.map((page) => { const Component = pageComponents?.[String(page.component)]; return <Route key={page.name} path={page.path} element={Component ? <Component /> : <main className="page"><div className="alert" role="alert">Page renderer unavailable</div></main>} />; })}
      <Route path="empty" element={<main className="page"><h1>No accessible resources</h1></main>} /><Route path="organization" element={<OrganizationPage />} />
    </Route></Route></Route>
  </Routes></AuthProvider>;
}

function HomeRedirect() {
  const first = useVisibleResources(resources)[0];
  const canReadAudit = useCanAccessRule(auditAccess);
  return <Navigate to={`/${first?.slug ?? customPages[0]?.path ?? (canReadAudit ? "audit" : "empty")}`} replace />;
}
