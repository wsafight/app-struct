import { Building2, Plus } from "lucide-react";
import { FormEvent, ReactNode, createContext, useContext, useEffect, useMemo, useState } from "react";
import { Outlet } from "react-router-dom";
import { tenantApi, type TenantOrganization } from "../generated/client";
import { errorMessage } from "../resource";

interface TenantContextValue {
  loading: boolean;
  organizations: TenantOrganization[];
  current: TenantOrganization | null;
  create(name: string): Promise<void>;
  select(id: string): void;
}

const TenantContext = createContext<TenantContextValue | null>(null);

export function TenantProvider({ children }: { children: ReactNode }) {
  const [loading, setLoading] = useState(true);
  const [organizations, setOrganizations] = useState<TenantOrganization[]>([]);
  const [current, setCurrent] = useState<TenantOrganization | null>(null);

  useEffect(() => {
    tenantApi.listOrganizations()
      .then(({ data }) => {
        setOrganizations(data);
        const selected = data.find((item) => item.id === tenantApi.current()) ?? data[0] ?? null;
        if (selected) tenantApi.select(selected.id); else tenantApi.clear();
        setCurrent(selected);
      })
      .catch(() => { setOrganizations([]); setCurrent(null); })
      .finally(() => setLoading(false));
  }, []);

  const value = useMemo<TenantContextValue>(() => ({
    loading,
    organizations,
    current,
    async create(name) {
      const organization = await tenantApi.createOrganization(name);
      tenantApi.select(organization.id);
      setOrganizations((items) => [...items, organization].sort((a, b) => a.name.localeCompare(b.name)));
      setCurrent(organization);
    },
    select(id) {
      if (id === current?.id) return;
      tenantApi.select(id);
      window.location.reload();
    },
  }), [current, loading, organizations]);

  return <TenantContext.Provider value={value}>{children}</TenantContext.Provider>;
}

export function useTenant(): TenantContextValue {
  const value = useContext(TenantContext);
  if (!value) throw new Error("TenantProvider is missing");
  return value;
}

export function RequireTenant() {
  const tenant = useTenant();
  if (tenant.loading) return <div className="auth-loading" aria-label="Loading" />;
  return tenant.current ? <Outlet /> : <TenantOnboarding />;
}

export function TenantSwitcher() {
  const tenant = useTenant();
  async function create() {
    const name = window.prompt("Organization name")?.trim();
    if (name) {
      await tenant.create(name);
      window.location.reload();
    }
  }
  return <div className="tenant-switcher">
    <Building2 size={16} aria-hidden />
    <select aria-label="Current organization" value={tenant.current?.id ?? ""} onChange={(event) => tenant.select(event.target.value)}>
      {tenant.organizations.map((organization) => <option key={organization.id} value={organization.id}>{organization.name}</option>)}
    </select>
    <button type="button" title="Create organization" aria-label="Create organization" onClick={() => void create()}><Plus size={15} /></button>
  </div>;
}

function TenantOnboarding() {
  const tenant = useTenant();
  const [name, setName] = useState("");
  const [error, setError] = useState("");
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    try { await tenant.create(name); } catch (reason) { setError(errorMessage(reason)); }
  }
  return <main className="auth-page"><div className="auth-panel">
    <div className="auth-brand">AppStruct</div><h1>Create organization</h1>
    {error && <div className="alert" role="alert">{error}</div>}
    <form className="auth-form" onSubmit={(event) => void submit(event)}><label>Name<input value={name} maxLength={120} onChange={(event) => setName(event.target.value)} required /></label><button className="primary-button"><Building2 size={17} /> Create</button></form>
  </div></main>;
}
