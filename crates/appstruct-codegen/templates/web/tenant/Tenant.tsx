import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Building2, MailPlus, Plus, Trash2, Users } from "lucide-react";
import {
  type FormEvent,
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Link, Outlet, useSearchParams } from "../navigation";
import { PromptDialog } from "../components/Dialog";
import {
  tenantApi,
  tenantStorageKey,
  type TenantInvitation,
  type TenantOrganization,
} from "../generated/client";
import { appQueryKeys } from "../query";
import { errorMessage } from "../resource";

interface TenantContextValue {
  loading: boolean;
  organizations: TenantOrganization[];
  current: TenantOrganization | null;
  create(name: string): Promise<void>;
  select(id: string): void;
}

const TenantContext = createContext<TenantContextValue | null>(null);
const EMPTY_ORGANIZATIONS: TenantOrganization[] = [];

export function TenantProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const [selectedId, setSelectedId] = useState(() => tenantApi.current() ?? "");
  const organizationsQuery = useQuery({
    queryKey: appQueryKeys.tenant.organizations,
    queryFn: ({ signal }) => tenantApi.listOrganizations({ signal }),
  });
  const organizations = organizationsQuery.data?.data ?? EMPTY_ORGANIZATIONS;
  const current =
    organizations.find((item) => item.id === selectedId) ??
    organizations[0] ??
    null;

  const clearTenantQueries = useCallback(() => {
    void queryClient.cancelQueries();
    queryClient.removeQueries({
      predicate: (query) => {
        const root = query.queryKey[0];
        return root !== appQueryKeys.session[0] && root !== "tenant";
      },
    });
  }, [queryClient]);

  const createOrganization = useMutation({
    mutationFn: tenantApi.createOrganization,
    onSuccess: (organization) => {
      tenantApi.select(organization.id);
      setSelectedId(organization.id);
      queryClient.setQueryData(
        appQueryKeys.tenant.organizations,
        (previous: { data: TenantOrganization[] } | undefined) => ({
          data: [...(previous?.data ?? []), organization].sort((a, b) =>
            a.name.localeCompare(b.name),
          ),
        }),
      );
      clearTenantQueries();
    },
  });

  useEffect(() => {
    if (organizationsQuery.isSuccess) {
      if (current) {
        tenantApi.select(current.id);
        if (current.id !== selectedId) setSelectedId(current.id);
      } else {
        tenantApi.clear();
        if (selectedId) setSelectedId("");
      }
    }
    const handleStorage = (event: StorageEvent) => {
      if (event.key !== tenantStorageKey) return;
      setSelectedId(tenantApi.current() ?? "");
      clearTenantQueries();
      void queryClient.invalidateQueries({
        queryKey: appQueryKeys.tenant.organizations,
      });
    };
    window.addEventListener("storage", handleStorage);
    return () => window.removeEventListener("storage", handleStorage);
  }, [
    clearTenantQueries,
    current,
    organizationsQuery.isSuccess,
    queryClient,
    selectedId,
  ]);

  const value = useMemo<TenantContextValue>(
    () => ({
      loading: organizationsQuery.isPending,
      organizations,
      current,
      async create(name) {
        await createOrganization.mutateAsync(name);
      },
      select(id) {
        if (id === current?.id) return;
        tenantApi.select(id);
        setSelectedId(id);
        clearTenantQueries();
      },
    }),
    [
      clearTenantQueries,
      createOrganization,
      current,
      organizations,
      organizationsQuery.isPending,
    ],
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
  const [creating, setCreating] = useState(false);
  return (
    <>
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
          onClick={() => setCreating(true)}
        >
          <Plus size={15} />
        </button>
      </div>
      <PromptDialog
        open={creating}
        title="Create organization"
        label="Organization name"
        confirmLabel="Create"
        onCancel={() => setCreating(false)}
        onConfirm={async (name) => {
          await tenant.create(name);
          setCreating(false);
        }}
      />
    </>
  );
}

export function OrganizationPage() {
  const tenant = useTenant();
  const queryClient = useQueryClient();
  const [email, setEmail] = useState("");
  const owner = tenant.current?.role === "owner";
  const queryKey = appQueryKeys.tenant.invitations(tenant.current?.id ?? "");
  const invitationsQuery = useQuery({
    queryKey,
    queryFn: ({ signal }) => tenantApi.listInvitations({ signal }),
    enabled: owner,
  });
  const invitations = invitationsQuery.data?.data ?? [];
  const inviteMutation = useMutation({
    mutationFn: (invitationEmail: string) => tenantApi.invite(invitationEmail),
    onSuccess: (invitation) => {
      queryClient.setQueryData(
        queryKey,
        (previous: { data: TenantInvitation[] } | undefined) => ({
          data: [invitation, ...(previous?.data ?? [])],
        }),
      );
      setEmail("");
    },
  });
  const revokeMutation = useMutation({
    mutationFn: tenantApi.revokeInvitation,
    onSuccess: (_, id) => {
      queryClient.setQueryData(
        queryKey,
        (previous: { data: TenantInvitation[] } | undefined) => ({
          data: (previous?.data ?? []).filter((item) => item.id !== id),
        }),
      );
    },
  });
  const requestError =
    inviteMutation.error ?? revokeMutation.error ?? invitationsQuery.error;
  const error = requestError ? errorMessage(requestError) : "";

  async function invite(event: FormEvent) {
    event.preventDefault();
    try {
      await inviteMutation.mutateAsync(email);
    } catch {
      // The mutation state renders the request error.
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
              <button
                className="primary-button"
                disabled={inviteMutation.isPending}
              >
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
                          disabled={revokeMutation.isPending}
                          onClick={() => revokeMutation.mutate(invitation.id)}
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
        <div className="auth-brand">__APP_TITLE__</div>
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
        <div className="auth-brand">__APP_TITLE__</div>
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
