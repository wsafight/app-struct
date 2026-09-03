import { Boxes, LogOut__HISTORY_ICON__ } from "lucide-react";
import { NavLink, Outlet, useNavigate } from "../navigation";
import { useAuth } from "../auth/Auth";
__AUDIT_RESOURCE_IMPORT__
import { type ResourceDefinition, __RESOURCE_HOOKS__ } from "../resource";
__TENANT_IMPORT__

export function Layout({ resources, pages }: { resources: ResourceDefinition[]; pages: readonly { name: string; label: string; path: string }[] }) {
  const auth = useAuth();
  const navigate = useNavigate();
  const visibleResources = useVisibleResources(resources);
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
__AUDIT_ACCESS__
  async function logout() {
    await auth.logout();
    navigate("/login", { replace: true });
  }
  return <div className="shell">
    <aside className="sidebar">
      <div className="brand"><Boxes size={20} aria-hidden /> <span>__APP_TITLE__</span></div>
__TENANT_SWITCHER__
      <nav aria-label="Resources">
        {visibleResources.map((resource) => <NavLink key={resource.name} to={`/${resource.slug}`}>{resource.label}</NavLink>)}
        {pages.map((page) => <NavLink key={page.name} to={`/${page.path}`}>{page.label}</NavLink>)}
        <NavLink to="/tokens">API tokens</NavLink>
        {isAdmin && <NavLink to="/admin">Administration</NavLink>}
__AUDIT_LINK__
      </nav>
      <div className="sidebar-account">
        <div className="account">
          <span>{auth.user?.email}</span>
          <span className="role-label">{auth.user?.roles.join(", ")}</span>
        </div>
        <button type="button" className="icon-button" title="Sign out" aria-label="Sign out" onClick={() => void logout()}><LogOut size={17} /></button>
      </div>
    </aside>
    <div className="workspace">
      <header className="topbar"><div className="account"><span>{auth.user?.email}</span><span className="role-label">{auth.user?.roles.join(", ")}</span></div><button type="button" className="icon-button" title="Sign out" aria-label="Sign out" onClick={() => void logout()}><LogOut size={17} /></button></header>
      <Outlet />
    </div>
  </div>;
}
