import { AppBrand, uiText } from "../ui";
import {
  ChevronRight,
  KeyRound,
  LayoutDashboard,
  LogOut,
  Settings2,
  Table2__HISTORY_ICON____REPORT_ICON____BILLING_ICON__,
} from "lucide-react";
import { NavLink, Outlet, useLocation } from "../navigation";
import { useAuth } from "../auth/Auth";
__AUDIT_RESOURCE_IMPORT__;
import { type ResourceDefinition, __RESOURCE_HOOKS__ } from "../resource";
__TENANT_IMPORT__;

export function Layout({
  resources,
  pages,
}: {
  resources: ResourceDefinition[];
  pages: readonly { name: string; label: string; path: string }[];
}) {
  const auth = useAuth();
  const visibleResources = useVisibleResources(resources);
  const { pathname } = useLocation();
  const systemPages: Record<string, string> = {
    tokens: uiText("apiTokens"),
    admin: uiText("administration"),
    audit: uiText("auditLog"),
    reports: uiText("reports"),
    billing: uiText("billing"),
    organization: uiText("organization"),
  };
  const section =
    visibleResources.find(
      (resource) => pathname.split("/")[1] === resource.slug,
    )?.label ??
    pages.find((page) => pathname === `/${page.path}`)?.label ??
    systemPages[pathname.split("/")[1]] ??
    uiText("workspace");
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  __AUDIT_ACCESS__;
  async function logout() {
    await auth.logout();
  }
  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <AppBrand fallbackTitle="__APP_TITLE__" />
        </div>
        __TENANT_SWITCHER__
        <nav aria-label={uiText("resources")}>
          <span className="nav-heading">{uiText("workspace")}</span>
          {visibleResources.map((resource) => (
            <NavLink key={resource.name} to={`/${resource.slug}`}>
              <Table2 size={17} aria-hidden />
              <span>{resource.label}</span>
            </NavLink>
          ))}
          {pages.map((page) => (
            <NavLink key={page.name} to={`/${page.path}`}>
              <LayoutDashboard size={17} aria-hidden />
              <span>{page.label}</span>
            </NavLink>
          ))}
          <span className="nav-heading">{uiText("tools")}</span>
          <NavLink to="/tokens">
            <KeyRound size={17} aria-hidden /> {uiText("apiTokens")}
          </NavLink>
          {isAdmin && (
            <NavLink to="/admin">
              <Settings2 size={17} aria-hidden /> {uiText("administration")}
            </NavLink>
          )}
          __AUDIT_LINK__ __REPORT_LINK__ __BILLING_LINK__
        </nav>
        <div className="sidebar-account">
          <span className="account-avatar" aria-hidden>
            {auth.user?.email.slice(0, 1).toUpperCase() || "A"}
          </span>
          <div className="account">
            <span>{auth.user?.email}</span>
            <span className="role-label">{auth.user?.roles.join(", ")}</span>
          </div>
          <button
            type="button"
            className="icon-button"
            title={uiText("signOut")}
            aria-label={uiText("signOut")}
            onClick={() => void logout()}
          >
            <LogOut size={17} />
          </button>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <div className="workspace-breadcrumb">
            <span>{uiText("workspace")}</span>
            <ChevronRight size={14} aria-hidden />
            <strong>{section}</strong>
          </div>
          {auth.user?.roles.length ? (
            <span className="workspace-role">{auth.user.roles.join(", ")}</span>
          ) : null}
        </header>
        <Outlet />
      </div>
    </div>
  );
}
