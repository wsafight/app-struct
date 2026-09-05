__REACT_IMPORT__
import { AuthProvider, RequireAuth } from "../auth/Auth";
__RESOURCE_IMPORT__
import { customPages } from "../generated/registry";
import type { AppStructRegistry, PageComponentProps } from "../generated/registry";
import { Navigate, Outlet, RuntimeRouter, createRuntimeRouter, type RuntimeRoute } from "../navigation";
__RESOURCE_ACCESS_IMPORT__
__TENANT_IMPORT__
import { Layout } from "./Layout";
import { resourceRoutes } from "./ResourceRoutes";

__AUDIT_PAGE__
const LoginPage = lazy(() => import("../auth/AuthPages").then(({ LoginPage: component }) => ({ default: component })));
const RegisterPage = lazy(() => import("../auth/AuthPages").then(({ RegisterPage: component }) => ({ default: component })));
const ForgotPasswordPage = lazy(() => import("../auth/AuthPages").then(({ ForgotPasswordPage: component }) => ({ default: component })));
const ResetPasswordPage = lazy(() => import("../auth/AuthPages").then(({ ResetPasswordPage: component }) => ({ default: component })));
const VerifyEmailPage = lazy(() => import("../auth/AuthPages").then(({ VerifyEmailPage: component }) => ({ default: component })));
const ApiTokensPage = lazy(() => import("../auth/AuthPages").then(({ ApiTokensPage: component }) => ({ default: component })));
const AdminPage = lazy(() => import("../auth/AuthPages").then(({ AdminPage: component }) => ({ default: component })));
const AdminUsersPage = lazy(() => import("../auth/AuthPages").then(({ AdminUsersPage: component }) => ({ default: component })));
const AdminJobsPage = lazy(() => import("../auth/AuthPages").then(({ AdminJobsPage: component }) => ({ default: component })));
const AdminWebhooksPage = lazy(() => import("../auth/AuthPages").then(({ AdminWebhooksPage: component }) => ({ default: component })));
const AdminSchedulesPage = lazy(() => import("../auth/AdminSchedulesPage").then(({ AdminSchedulesPage: component }) => ({ default: component })));
const AdminMailPage = lazy(() => import("../auth/AdminStoragePages").then(({ AdminMailPage: component }) => ({ default: component })));
const AdminMailDetailPage = lazy(() => import("../auth/AdminStoragePages").then(({ AdminMailDetailPage: component }) => ({ default: component })));
const AdminFilesPage = lazy(() => import("../auth/AdminStoragePages").then(({ AdminFilesPage: component }) => ({ default: component })));
const AdminFileDetailPage = lazy(() => import("../auth/AdminStoragePages").then(({ AdminFileDetailPage: component }) => ({ default: component })));

export function App({ registry }: { registry?: AppStructRegistry }) {
  const [router] = useState(() => createRuntimeRouter(AuthRoot, appRoutes(registry)));
  return <RuntimeRouter router={router} />;
}

function AuthRoot() {
  return <AuthProvider><Outlet /></AuthProvider>;
}

__TENANT_ROOT__
function appRoutes(registry?: AppStructRegistry): RuntimeRoute[] {
  return [
    { path: "/login", component: LoginPage },
    { path: "/register", component: RegisterPage },
    { path: "/forgot-password", component: ForgotPasswordPage },
    { path: "/reset-password", component: ResetPasswordPage },
__INVITATION_ROUTE__
    { path: "/verify-email", component: VerifyEmailPage },
    {
      id: "_authenticated",
      component: RequireAuth,
      children: [authenticatedScope(registry)],
    },
  ];
}

function authenticatedScope(registry?: AppStructRegistry): RuntimeRoute {
  const layout: RuntimeRoute = {
    id: "_layout",
    component: () => <Layout resources={resources} pages={customPages} />,
    children: layoutRoutes(registry),
  };
  return __AUTHENTICATED_SCOPE__;
}

function layoutRoutes(registry?: AppStructRegistry): RuntimeRoute[] {
  return [
    { path: "/", component: HomeRedirect },
    ...resourceRoutes(registry),
__AUDIT_ROUTE__
    ...customPageRoutes(registry),
    { path: "/empty", component: EmptyPage },
__ORGANIZATION_ROUTE__
    { path: "/tokens", component: ApiTokensPage },
    { path: "/admin", component: AdminPage },
    { path: "/admin/users", component: AdminUsersPage },
    { path: "/admin/jobs", component: AdminJobsPage },
    { path: "/admin/webhooks", component: AdminWebhooksPage },
    { path: "/admin/schedules", component: AdminSchedulesPage },
    { path: "/admin/mail", component: AdminMailPage },
    { path: "/admin/mail/$id", component: AdminMailDetailPage },
    { path: "/admin/files", component: AdminFilesPage },
    { path: "/admin/files/$id", component: AdminFileDetailPage },
  ];
}

function HomeRedirect() {
  const first = useVisibleResources(resources)[0];
__HOME_REDIRECT__
}

function EmptyPage() {
  return <main className="page"><h1>No accessible resources</h1></main>;
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
