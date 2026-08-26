import { Boxes, LogOut } from "lucide-react";
import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { useAuth } from "../auth/Auth";
import { type ResourceDefinition, useVisibleResources } from "../resource";

export function Layout({ resources, pages }: { resources: ResourceDefinition[]; pages: readonly { name: string; label: string; path: string }[] }) {
  const auth = useAuth();
  const navigate = useNavigate();
  const visibleResources = useVisibleResources(resources);
  async function logout() {
    await auth.logout();
    navigate("/login", { replace: true });
  }
  return <div className="shell">
    <aside className="sidebar">
      <div className="brand"><Boxes size={20} aria-hidden /> <span>AppStruct</span></div>
      <nav aria-label="Resources">
        {visibleResources.map((resource) => <NavLink key={resource.name} to={`/${resource.slug}`}>{resource.label}</NavLink>)}
        {pages.map((page) => <NavLink key={page.name} to={`/${page.path}`}>{page.label}</NavLink>)}
      </nav>
    </aside>
    <div className="workspace">
      <header className="topbar"><div className="account"><span>{auth.user?.email}</span><span className="role-label">{auth.user?.roles.join(", ")}</span></div><button type="button" className="icon-button" title="Sign out" aria-label="Sign out" onClick={() => void logout()}><LogOut size={17} /></button></header>
      <Outlet />
    </div>
  </div>;
}
