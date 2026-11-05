import { AppBrand, formatNumber, uiText, t } from "../ui";
import {
  Bell,
  ChevronRight,
  Coins,
  KeyRound,
  LayoutDashboard,
  LogOut,
  Menu,
  Settings2,
  Table2__HISTORY_ICON____REPORT_ICON____BILLING_ICON____FILE_ICON__,
  UserRound,
  X,
} from "lucide-react";
import { useRef, useState } from "react";
import { NavLink, Outlet, useLocation } from "../navigation";
import { useAuth } from "../auth/Auth";
__AUDIT_RESOURCE_IMPORT__;
import { type ResourceDefinition, __RESOURCE_HOOKS__ } from "../resource";
__TENANT_IMPORT__;
import { productApi, productFeatures } from "../generated/client";
import { useQuery } from "@tanstack/react-query";
import { appQueryKeys } from "../query";

export function Layout({
  resources,
  pages,
}: {
  resources: ResourceDefinition[];
  pages: readonly { name: string; label: string; path: string }[];
}) {
  const auth = useAuth();
  const { pathname } = useLocation();
  const [openPath, setOpenPath] = useState<string | null>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const menuOpen = openPath === pathname;
  const notifications = useQuery({
    queryKey: appQueryKeys.product.notifications(1),
    queryFn: ({ signal }) => productApi.notifications(1, { signal }),
    enabled: Boolean(auth.user),
    refetchInterval: 30_000,
  });
  const visibleResources = useVisibleResources(resources);
  const systemPages: Record<string, string> = {
    tokens: uiText("apiTokens"),
    admin: uiText("administration"),
    audit: uiText("auditLog"),
    reports: uiText("reports"),
    results: t("Tasks and results"),
    files: t("Files"),
    notifications: t("Notifications"),
    team: t("Team settings"),
    credits: t("Credits"),
    billing: uiText("billing"),
    organization: uiText("organization"),
  };
  const section =
    visibleResources.find(
      (resource) => pathname.split("/")[1] === resource.slug,
    )?.label ??
    t(
      pages.find((page) => pathname === `/${page.path}`)?.label ??
        systemPages[pathname.split("/")[1]] ??
        uiText("workspace"),
    );
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const resourceActive = visibleResources.some(
    (resource) => pathname.split("/")[1] === resource.slug,
  );
  const advancedActive = ["tokens", "audit", "admin"].includes(
    pathname.split("/")[1],
  );
  __AUDIT_ACCESS__;
  return (
    <div className="shell">
      <aside
        className="sidebar"
        onKeyDown={(event) => {
          if (event.key === "Escape" && menuOpen) {
            setOpenPath(null);
            toggle.current?.focus();
          }
        }}
      >
        <div className="sidebar-brand-row">
          <div className="brand">
            <AppBrand fallbackTitle="__APP_TITLE__" />
          </div>
          <button
            ref={toggle}
            type="button"
            className="icon-button mobile-nav-toggle"
            aria-label={t("Toggle navigation")}
            aria-expanded={menuOpen}
            aria-controls="workspace-navigation"
            onClick={() => setOpenPath(menuOpen ? null : pathname)}
          >
            {menuOpen ? (
              <X size={20} aria-hidden />
            ) : (
              <Menu size={20} aria-hidden />
            )}
          </button>
        </div>
        __TENANT_SWITCHER__
        <nav
          id="workspace-navigation"
          className="workspace-navigation"
          data-open={menuOpen}
          aria-label={t("Main navigation")}
          onClick={(event) => {
            if (event.target instanceof Element && event.target.closest("a"))
              setOpenPath(null);
          }}
        >
          <div className="nav-section">
            <span className="nav-heading">{uiText("workspace")}</span>
            {pages.map((page) => (
              <NavLink key={page.name} to={`/${page.path}`}>
                <LayoutDashboard size={17} aria-hidden />
                <span>{t(page.label)}</span>
              </NavLink>
            ))}
            __REPORT_LINK__ __FILE_LINK__
            {visibleResources.length > 0 && (
              <details
                className="nav-group"
                open={resourceActive || pages.length === 0}
              >
                <summary>{t("Data records")}</summary>
                <div>
                  {visibleResources.map((resource) => (
                    <NavLink key={resource.name} to={`/${resource.slug}`}>
                      <Table2 size={17} aria-hidden />
                      <span>{resource.label}</span>
                    </NavLink>
                  ))}
                </div>
              </details>
            )}
          </div>
          <div className="nav-section">
            <span className="nav-heading">{t("Team and account")}</span>
            <NavLink to="/notifications">
              <Bell size={17} aria-hidden />
              <span>{t("Notifications")}</span>
              {Boolean(notifications.data?.unread) && (
                <span
                  className="notification-count"
                  aria-label={t("{0} unread notifications", {
                    "0": formatNumber(notifications.data!.unread),
                  })}
                >
                  {formatNumber(notifications.data!.unread)}
                </span>
              )}
            </NavLink>
            {productFeatures.team && (
              <>
                <NavLink to="/team">
                  <UserRound size={17} aria-hidden />
                  <span>{t("Team settings")}</span>
                </NavLink>
                <NavLink to="/credits">
                  <Coins size={17} aria-hidden />
                  <span>{t("Credits")}</span>
                </NavLink>
              </>
            )}
            __BILLING_LINK__
            <NavLink to="/account">
              <UserRound size={17} aria-hidden />
              <span>{uiText("account")}</span>
            </NavLink>
          </div>
          <details className="nav-group" open={advancedActive}>
            <summary>{t("Advanced tools")}</summary>
            <div>
              <NavLink to="/tokens">
                <KeyRound size={17} aria-hidden />
                <span>{uiText("apiTokens")}</span>
              </NavLink>
              __AUDIT_LINK__
              {isAdmin && (
                <>
                  <NavLink to="/admin">
                    <Settings2 size={17} aria-hidden />
                    <span>{uiText("administration")}</span>
                  </NavLink>
                  <NavLink to="/admin/insights">
                    <LayoutDashboard size={17} aria-hidden />
                    <span>{t("Business insights")}</span>
                  </NavLink>
                </>
              )}
            </div>
          </details>
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
            onClick={() => void auth.logout()}
          >
            <LogOut size={17} aria-hidden />
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
