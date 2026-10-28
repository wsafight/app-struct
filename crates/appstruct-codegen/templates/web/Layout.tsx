import { AppBrand, uiText } from "../ui";
import { ChevronRight, LayoutDashboard, Table2 } from "lucide-react";
import { NavLink, Outlet, useLocation } from "../navigation";
import { type ResourceDefinition, useVisibleResources } from "../resource";

export function Layout({
  resources,
  pages,
}: {
  resources: ResourceDefinition[];
  pages: readonly { name: string; label: string; path: string }[];
}) {
  const visibleResources = useVisibleResources(resources);
  const { pathname } = useLocation();
  const section =
    visibleResources.find(
      (resource) => pathname.split("/")[1] === resource.slug,
    )?.label ??
    pages.find((page) => pathname === `/${page.path}`)?.label ??
    uiText("workspace");
  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <AppBrand fallbackTitle="__APP_TITLE__" />
        </div>
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
        </nav>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <div className="workspace-breadcrumb">
            <span>{uiText("workspace")}</span>
            <ChevronRight size={14} aria-hidden />
            <strong>{section}</strong>
          </div>
        </header>
        <Outlet />
      </div>
    </div>
  );
}
