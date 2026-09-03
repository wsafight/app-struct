import { Building2, MailPlus, Plus, Trash2, Users } from "lucide-react";
import {
  FormEvent,
  ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Link, Outlet, useSearchParams } from "../navigation";
import {
  tenantApi,
  tenantStorageKey,
  type TenantInvitation,
  type TenantOrganization,
} from "../generated/client";
import { queryClient } from "../query";
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

  const loadOrganizations = useCallback(() => {
    setLoading(true);
    tenantApi
      .listOrganizations()
      .then(({ data }) => {
        setOrganizations(data);
        const selected =
          data.find((item) => item.id === tenantApi.current()) ??
          data[0] ??
          null;
        if (selected) tenantApi.select(selected.id);
        else tenantApi.clear();
        setCurrent(selected);
      })
      .catch(() => {
        setOrganizations([]);
        setCurrent(null);
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    loadOrganizations();
    const handleStorage = (event: StorageEvent) => {
      if (event.key !== tenantStorageKey) return;
      void queryClient.cancelQueries();
      queryClient.clear();
      loadOrganizations();
    };
    window.addEventListener("storage", handleStorage);
    return () => window.removeEventListener("storage", handleStorage);
  }, [loadOrganizations]);

  const value = useMemo<TenantContextValue>(
    () => ({
      loading,
      organizations,
      current,
      async create(name) {
        const organization = await tenantApi.createOrganization(name);
        tenantApi.select(organization.id);
        void queryClient.cancelQueries();
        queryClient.clear();
        setOrganizations((items) =>
          [...items, organization].sort((a, b) => a.name.localeCompare(b.name)),
        );
        setCurrent(organization);
      },
      select(id) {
        if (id === current?.id) return;
        tenantApi.select(id);
        void queryClient.cancelQueries();
        queryClient.clear();
        setCurrent(
          organizations.find((organization) => organization.id === id) ?? null,
        );
      },
    }),
    [current, loading, organizations],
  );

  return (
    <TenantContext.Provider value={value}>{children}</TenantContext.Provider>
  );
}

export function useTenant(): TenantContextValue {
  const value = useContext(TenantContext);
  if (!value) throw new Error("TenantProvider is missing");
  return value;
}

export function RequireTenant() {
  const tenant = useTenant();
  if (tenant.loading)
    return <div className="auth-loading" aria-label="Loading" />;
  return tenant.current ? <Outlet /> : <TenantOnboarding />;
}

export function TenantSwitcher() {
  const tenant = useTenant();
  async function create() {
    const name = window.prompt("Organization name")?.trim();
    if (name) {
      await tenant.create(name);
    }
  }
  return (
    <div className="tenant-switcher">
      <Building2 size={16} aria-hidden />
      <select
        aria-label="Current organization"
        value={tenant.current?.id ?? ""}
        onChange={(event) => tenant.select(event.target.value)}
      >
        {tenant.organizations.map((organization) => (
          <option key={organization.id} value={organization.id}>
            {organization.name}
          </option>
        ))}
      </select>
      <Link
        to="/organization"
        title="Organization settings"
        aria-label="Organization settings"
      >
        <Users size={15} />
      </Link>
      <button
        type="button"
        title="Create organization"
        aria-label="Create organization"
        onClick={() => void create()}
      >
        <Plus size={15} />
      </button>
    </div>
  );
}

export function OrganizationPage() {
  const tenant = useTenant();
  const [invitations, setInvitations] = useState<TenantInvitation[]>([]);
  const [email, setEmail] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const owner = tenant.current?.role === "owner";

  useEffect(() => {
    if (owner)
      tenantApi
        .listInvitations()
        .then(({ data }) => setInvitations(data))
        .catch((reason) => setError(errorMessage(reason)));
  }, [owner, tenant.current?.id]);

  async function invite(event: FormEvent) {
    event.preventDefault();
    setError("");
    setBusy(true);
    try {
      const invitation = await tenantApi.invite(email);
      setInvitations((items) => [invitation, ...items]);
      setEmail("");
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function revoke(id: string) {
    setError("");
    try {
      await tenantApi.revokeInvitation(id);
      setInvitations((items) => items.filter((item) => item.id !== id));
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <h1>Organization</h1>
          <p>Manage members and invitations for {tenant.current?.name}.</p>
        </div>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      {owner ? (
        <>
          <section className="form-frame invitation-form">
            <h2>Invite a member</h2>
            <form className="toolbar" onSubmit={(event) => void invite(event)}>
              <label className="sr-only" htmlFor="invitation-email">
                Email
              </label>
              <input
                id="invitation-email"
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                placeholder="name@example.com"
                required
              />
              <button className="primary-button" disabled={busy}>
                <MailPlus size={16} /> Invite
              </button>
            </form>
          </section>
          <section className="table-frame invitation-list">
            <table>
              <thead>
                <tr>
                  <th>Email</th>
                  <th>Role</th>
                  <th>Status</th>
                  <th>Expires</th>
                  <th aria-label="Actions" />
                </tr>
              </thead>
              <tbody>
                {invitations.map((invitation) => (
                  <tr key={invitation.id}>
                    <td>{invitation.email}</td>
                    <td>{invitation.role}</td>
                    <td>{invitation.accepted_at ? "Accepted" : "Pending"}</td>
                    <td>
                      {new Date(invitation.expires_at).toLocaleDateString()}
                    </td>
                    <td>
                      {!invitation.accepted_at && (
                        <button
                          type="button"
                          className="icon-button danger"
                          title="Revoke invitation"
                          aria-label={`Revoke invitation for ${invitation.email}`}
                          onClick={() => void revoke(invitation.id)}
                        >
                          <Trash2 size={15} />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {invitations.length === 0 && (
              <div className="empty">No invitations yet</div>
            )}
          </section>
        </>
      ) : (
        <div className="alert" role="status">
          Only organization owners can manage invitations.
        </div>
      )}
    </main>
  );
}

export function InvitationAcceptPage() {
  const [params] = useSearchParams();
  const [status, setStatus] = useState<"loading" | "accepted" | "error">(
    "loading",
  );
  const [message, setMessage] = useState("");
  const requestedToken = useRef<string | null>(null);
  const token = params.get("token");
  useEffect(() => {
    if (!token) {
      setStatus("error");
      setMessage("This invitation link is missing its token.");
      return;
    }
    if (requestedToken.current === token) return;
    requestedToken.current = token;
    let active = true;
    tenantApi
      .acceptInvitation(token)
      .then((organization) => {
        if (!active) return;
        tenantApi.select(organization.id);
        setStatus("accepted");
        setMessage(`You joined ${organization.name}.`);
        window.history.replaceState({}, "", "/organization");
      })
      .catch((reason) => {
        if (active) {
          setStatus("error");
          setMessage(errorMessage(reason));
        }
      });
    return () => {
      active = false;
    };
  }, [token]);
  return (
    <main className="auth-page">
      <div className="auth-panel">
        <div className="auth-brand">AppStruct</div>
        <h1>
          {status === "loading"
            ? "Accepting invitation"
            : status === "accepted"
              ? "Invitation accepted"
              : "Invitation unavailable"}
        </h1>
        {status === "loading" ? (
          <div className="auth-loading" aria-label="Loading" />
        ) : status === "accepted" ? (
          <>
            <p>{message}</p>
            <Link className="primary-button" to="/organization">
              Open organization
            </Link>
          </>
        ) : (
          <div className="alert" role="alert">
            {message}
          </div>
        )}
      </div>
    </main>
  );
}

function TenantOnboarding() {
  const tenant = useTenant();
  const [name, setName] = useState("");
  const [error, setError] = useState("");
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    try {
      await tenant.create(name);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }
  return (
    <main className="auth-page">
      <div className="auth-panel">
        <div className="auth-brand">AppStruct</div>
        <h1>Create organization</h1>
        {error && (
          <div className="alert" role="alert">
            {error}
          </div>
        )}
        <form className="auth-form" onSubmit={(event) => void submit(event)}>
          <label>
            Name
            <input
              value={name}
              maxLength={120}
              onChange={(event) => setName(event.target.value)}
              required
            />
          </label>
          <button className="primary-button">
            <Building2 size={17} /> Create
          </button>
        </form>
      </div>
    </main>
  );
}
