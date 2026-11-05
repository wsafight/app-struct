import { t } from "../ui";
__REACT_IMPORT__
import { AuthProvider, RequireAuth } from "../auth/Auth";
__RESOURCE_IMPORT__
import { customPages, workspacePath, type AppStructRegistry } from "../generated/registry";
import { Navigate, Outlet, RuntimeRouter, createRuntimeRouter, type RuntimeRoute } from "../navigation";
__RESOURCE_ACCESS_IMPORT__
__TENANT_IMPORT__
import { Layout } from "./Layout";
import { resourceRoutes } from "./ResourceRoutes";
import { customPageRoutes, PageScopeLayout, workspacePages } from "./PageRoutes";

__AUDIT_PAGE__
__REPORT_PAGE__
__BILLING_PAGE__
__BILLING_ADMIN_PAGE__
__FILE_PAGE__
const NotificationsPage = lazy(() => import("../product/ProductPages").then(({ NotificationsPage: component }) => ({ default: component })));
const InsightsPage = lazy(() => import("../product/ProductPages").then(({ InsightsPage: component }) => ({ default: component })));
__PRODUCT_PAGES__
const LoginPage = lazy(() => import("../auth/PublicAuthPages").then(({ LoginPage: component }) => ({ default: component })));
const RegisterPage = lazy(() => import("../auth/PublicAuthPages").then(({ RegisterPage: component }) => ({ default: component })));
const ForgotPasswordPage = lazy(() => import("../auth/PublicAuthPages").then(({ ForgotPasswordPage: component }) => ({ default: component })));
const ResetPasswordPage = lazy(() => import("../auth/PublicAuthPages").then(({ ResetPasswordPage: component }) => ({ default: component })));
const VerifyEmailPage = lazy(() => import("../auth/PublicAuthPages").then(({ VerifyEmailPage: component }) => ({ default: component })));
const AccountPage = lazy(() => import("../auth/AccountPage").then(({ AccountPage: component }) => ({ default: component })));
const BusinessPage = lazy(() => import("../business/BusinessPage").then(({ BusinessPage: component }) => ({ default: component })));
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
    { id: "_public", component: () => <PageScopeLayout registry={registry} scope="public" />, children: customPageRoutes(registry, "public") },
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
    component: () => registry?.layouts?.workspace
      ? <PageScopeLayout registry={registry} scope="workspace" />
      : <Layout resources={resources} pages={workspacePages} />,
    children: layoutRoutes(registry),
  };
  const account: RuntimeRoute = {
    id: "_account",
    component: () => <PageScopeLayout registry={registry} scope="account" />,
    children: [
      { path: "/notifications", component: NotificationsPage },
      { path: "/admin/insights", component: InsightsPage },
      ...customPageRoutes(registry, "account"),
      ...(!customPages.some((page) => page.path === "account") ? [{ path: "/account", component: AccountPage }] : []),
__ONBOARDING_ROUTE__
    ],
  };
  return __AUTHENTICATED_SCOPE__;
}

function layoutRoutes(registry?: AppStructRegistry): RuntimeRoute[] {
  return [
    { path: workspacePath, component: HomeRedirect },
    ...resourceRoutes(registry),
__AUDIT_ROUTE__
__REPORT_ROUTE__
__BILLING_ROUTE__
__FILE_ROUTE__
__PRODUCT_ROUTES__
    ...customPageRoutes(registry, "workspace"),
    { path: "/empty", component: EmptyPage },
__ORGANIZATION_ROUTE__
    { path: "/tokens", component: ApiTokensPage },
    { path: "/admin", component: AdminPage },
    { path: "/admin/business", component: BusinessPage },
    { path: "/admin/users", component: AdminUsersPage },
    { path: "/admin/jobs", component: AdminJobsPage },
    { path: "/admin/webhooks", component: AdminWebhooksPage },
    { path: "/admin/schedules", component: AdminSchedulesPage },
    { path: "/admin/mail", component: AdminMailPage },
    { path: "/admin/mail/$id", component: AdminMailDetailPage },
    { path: "/admin/files", component: AdminFilesPage },
    { path: "/admin/files/$id", component: AdminFileDetailPage },
__BILLING_ADMIN_ROUTE__
  ];
}

function HomeRedirect() {
  const first = useVisibleResources(resources)[0];
__HOME_REDIRECT__
}

function EmptyPage() {
  return <main className="page"><h1>{t("No accessible resources")}</h1></main>;
}
