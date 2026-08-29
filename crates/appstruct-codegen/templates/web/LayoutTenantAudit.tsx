import { Boxes, History, LogOut } from "lucide-react";
import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { useAuth } from "../auth/Auth";
import { auditAccess } from "../generated/resources";
import { type ResourceDefinition, useCanAccessRule, useVisibleResources } from "../resource";
import { TenantSwitcher } from "../tenant/Tenant";

export function Layout({ resources, pages }: { resources: ResourceDefinition[]; pages: readonly { name: string; label: string; path: string }[] }) {
  const auth = useAuth(); const navigate = useNavigate();
  const visibleResources = useVisibleResources(resources); const canReadAudit = useCanAccessRule(auditAccess); const isAdmin = auth.user?.roles.includes("admin") ?? false;
  async function logout() { await auth.logout(); navigate("/login", { replace: true }); }
  return <div className="shell"><aside className="sidebar"><div className="brand"><Boxes size={20} aria-hidden /> <span>AppStruct</span></div><TenantSwitcher /><nav aria-label="Resources">{visibleResources.map((resource) => <NavLink key={resource.name} to={`/${resource.slug}`}>{resource.label}</NavLink>)}{pages.map((page) => <NavLink key={page.name} to={`/${page.path}`}>{page.label}</NavLink>)}<NavLink to="/tokens">API tokens</NavLink>{isAdmin && <NavLink to="/admin">Administration</NavLink>}{canReadAudit && <NavLink to="/audit"><History size={15} /> Audit log</NavLink>}</nav></aside><div className="workspace"><header className="topbar"><div className="account"><span>{auth.user?.email}</span><span className="role-label">{auth.user?.roles.join(", ")}</span></div><button type="button" className="icon-button" title="Sign out" aria-label="Sign out" onClick={() => void logout()}><LogOut size={17} /></button></header><Outlet /></div></div>;
}
