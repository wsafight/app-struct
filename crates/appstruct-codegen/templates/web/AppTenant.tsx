import { useState, type ComponentType } from "react";
import { AuthProvider, RequireAuth } from "../auth/Auth";
import { AdminJobsPage, AdminPage, AdminUsersPage, ApiTokensPage, ForgotPasswordPage, LoginPage, RegisterPage, ResetPasswordPage, VerifyEmailPage } from "../auth/AuthPages";
import { resources } from "../generated/resources";
import { customPages } from "../generated/registry";
import type { AppStructRegistry, PageComponentProps } from "../generated/registry";
import { Navigate, Outlet, RuntimeRouter, createRuntimeRouter, type RuntimeRoute } from "../navigation";
import { ResourceDetail } from "../pages/ResourceDetail";
import { ResourceForm } from "../pages/ResourceForm";
import { ResourceList } from "../pages/ResourceList";
import { useVisibleResources } from "../resource";
import { InvitationAcceptPage, OrganizationPage, RequireTenant, TenantProvider } from "../tenant/Tenant";
import { Layout } from "./Layout";

export function App({ registry }: { registry?: AppStructRegistry }) {
  const [router] = useState(() => createRuntimeRouter(AuthRoot, appRoutes(registry)));
  return <RuntimeRouter router={router} />;
}

function AuthRoot() {
  return <AuthProvider><Outlet /></AuthProvider>;
}

function TenantRoot() {
  return <TenantProvider><RequireTenant /></TenantProvider>;
}

function appRoutes(registry?: AppStructRegistry): RuntimeRoute[] {
  const pageComponents = registry?.pages as Record<string, ComponentType<PageComponentProps>> | undefined;
  return [
    { path: "/login", component: LoginPage },
    { path: "/register", component: RegisterPage },
    { path: "/forgot-password", component: ForgotPasswordPage },
    { path: "/reset-password", component: ResetPasswordPage },
    { path: "/accept-invitation", component: InvitationAcceptPage },
    { path: "/verify-email", component: VerifyEmailPage },
    {
      id: "_authenticated",
      component: RequireAuth,
      children: [{
        id: "_tenant",
        component: TenantRoot,
        children: [{
          id: "_layout",
          component: () => <Layout resources={resources} pages={customPages} />,
          children: [
            { path: "/", component: HomeRedirect },
            ...resourceRoutes(registry),
            ...customPages.map((page) => ({ path: absolutePath(page.path), component: pageComponents?.[String(page.component)] ?? PageRendererUnavailable })),
            { path: "/empty", component: EmptyPage },
            { path: "/organization", component: OrganizationPage },
            { path: "/tokens", component: ApiTokensPage },
            { path: "/admin", component: AdminPage },
            { path: "/admin/users", component: AdminUsersPage },
            { path: "/admin/jobs", component: AdminJobsPage },
          ],
        }],
      }],
    },
  ];
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
