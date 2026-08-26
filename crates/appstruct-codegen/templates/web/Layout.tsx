import { Boxes } from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";
import type { ResourceDefinition } from "../resource";

export function Layout({ resources, pages }: { resources: ResourceDefinition[]; pages: readonly { name: string; label: string; path: string }[] }) {
  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand"><Boxes size={20} aria-hidden /> <span>AppStruct</span></div>
        <nav aria-label="Resources">
          {resources.map((resource) => (
            <NavLink key={resource.name} to={`/${resource.slug}`}>
              {resource.label}
            </NavLink>
          ))}
          {pages.map((page) => <NavLink key={page.name} to={`/${page.path}`}>{page.label}</NavLink>)}
        </nav>
      </aside>
      <div className="workspace">
        <header className="topbar"><span>Workspace</span><span className="environment">Local</span></header>
        <Outlet />
      </div>
    </div>
  );
}
