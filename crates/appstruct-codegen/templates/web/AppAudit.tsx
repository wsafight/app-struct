import { lazy, useState, type ComponentType } from "react";
import { AuthProvider, RequireAuth } from "../auth/Auth";
import { AdminJobsPage, AdminPage, AdminUsersPage, ApiTokensPage, ForgotPasswordPage, LoginPage, RegisterPage, ResetPasswordPage, VerifyEmailPage } from "../auth/AuthPages";
import { auditAccess, resources } from "../generated/resources";
import { customPages } from "../generated/registry";
import type { AppStructRegistry, PageComponentProps } from "../generated/registry";
import { Navigate, Outlet, RuntimeRouter, createRuntimeRouter, type RuntimeRoute } from "../navigation";
import { useCanAccessRule, useVisibleResources } from "../resource";
import { Layout } from "./Layout";

const AuditPage = lazy(() => import("../audit/AuditPage").then(({ AuditPage: component }) => ({ default: component })));
const ResourceDetail = lazy(() => import("../pages/ResourceDetail").then(({ ResourceDetail: component }) => ({ default: component })));
const ResourceForm = lazy(() => import("../pages/ResourceForm").then(({ ResourceForm: component }) => ({ default: component })));
const ResourceList = lazy(() => import("../pages/ResourceList").then(({ ResourceList: component }) => ({ default: component })));

export function App({ registry }: { registry?: AppStructRegistry }) {
  const [router] = useState(() => createRuntimeRouter(AuthRoot, appRoutes(registry)));
  return <RuntimeRouter router={router} />;
}

function AuthRoot() {
  return <AuthProvider><Outlet /></AuthProvider>;
}

function appRoutes(registry?: AppStructRegistry): RuntimeRoute[] {
  const pageComponents = registry?.pages as Record<string, ComponentType<PageComponentProps>> | undefined;
  return [
    { path: "/login", component: LoginPage },
    { path: "/register", component: RegisterPage },
    { path: "/forgot-password", component: ForgotPasswordPage },
    { path: "/reset-password", component: ResetPasswordPage },
    { path: "/verify-email", component: VerifyEmailPage },
    {
      id: "_authenticated",
      component: RequireAuth,
      children: [{
        id: "_layout",
        component: () => <Layout resources={resources} pages={customPages} />,
        children: [
          { path: "/", component: HomeRedirect },
          ...resourceRoutes(registry),
          { path: "/audit", component: AuditPage },
          { path: "/tokens", component: ApiTokensPage },
            { path: "/admin", component: AdminPage },
            { path: "/admin/users", component: AdminUsersPage },
            { path: "/admin/jobs", component: AdminJobsPage },
          ...customPages.map((page) => ({ path: absolutePath(page.path), component: pageComponents?.[String(page.component)] ?? PageRendererUnavailable })),
          { path: "/empty", component: EmptyPage },
        ],
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
  const canReadAudit = useCanAccessRule(auditAccess);
  return <Navigate to={`/${first?.slug ?? customPages[0]?.path ?? (canReadAudit ? "audit" : "empty")}`} replace />;
}

function EmptyPage() {
  return <main className="page"><h1>No accessible resources</h1></main>;
}

function PageRendererUnavailable() {
  return <main className="page"><div className="alert" role="alert">Page renderer unavailable</div></main>;
}

function absolutePath(path: string): string {
  return `/${path.replace(/^\/+/, "")}`;
}
