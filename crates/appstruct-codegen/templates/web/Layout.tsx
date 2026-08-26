import { Boxes } from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";
import type { ResourceDefinition } from "../resource";

export function Layout({ resources }: { resources: ResourceDefinition[] }) {
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
        </nav>
      </aside>
      <div className="workspace">
        <header className="topbar"><span>Workspace</span><span className="environment">Local</span></header>
        <Outlet />
      </div>
    </div>
  );
}

