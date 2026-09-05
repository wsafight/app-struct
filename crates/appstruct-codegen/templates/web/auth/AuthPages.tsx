import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  type FormEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  Copy,
  CopyPlus,
  KeyRound,
  LogIn,
  Mail,
  Plus,
  RotateCcw,
  Trash2,
  UserPlus,
} from "lucide-react";
import {
  Link,
  Navigate,
  useLocation,
  useNavigate,
  useSearchParams,
} from "../navigation";
import {
  adminApi,
  adminFeatures,
  authApi,
  authFeatures,
  type AdminJob,
  type AdminJobStatus,
  type AdminOverview,
  type AdminUser,
  type AdminWebhookDelivery,
  type AdminWebhookStatus,
  type ApiToken,
  type CreatedApiToken,
} from "../generated/client";
import { appQueryKeys } from "../query";
import { errorMessage } from "../resource";
import { useAuth } from "./Auth";

export function LoginPage() {
  return <CredentialsPage mode="login" />;
}

export function RegisterPage() {
  if (!authFeatures.registration) return <Navigate to="/login" replace />;
  return <CredentialsPage mode="register" />;
}

function CredentialsPage({ mode }: { mode: "login" | "register" }) {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const redirecting = useRef(false);

  useEffect(() => {
    if (!auth.user || submitting) {
      redirecting.current = false;
      return;
    }
    if (redirecting.current) return;
    redirecting.current = true;
    void navigate("/", { replace: true });
  }, [auth.user, navigate, submitting]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSubmitting(true);
    setError("");
    try {
      if (mode === "login") await auth.login(email, password);
      else await auth.register(email, password);
      const fromState = (
        location.state as {
          from?: { pathname?: string; searchStr?: string; hash?: string };
        } | null
      )?.from;
      const search = fromState?.searchStr
        ? fromState.searchStr.startsWith("?")
          ? fromState.searchStr
          : `?${fromState.searchStr}`
        : "";
      const hash = fromState?.hash
        ? fromState.hash.startsWith("#")
          ? fromState.hash
          : `#${fromState.hash}`
        : "";
      const from = fromState?.pathname
        ? `${fromState.pathname}${search}${hash}`
        : "/";
      await navigate(from, { replace: true });
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The request could not be completed",
      );
    } finally {
      setSubmitting(false);
    }
  }

  const registering = mode === "register";
  return (
    <AuthFrame title={registering ? "Create account" : "Sign in"}>
      <form className="auth-form" onSubmit={(event) => void submit(event)}>
        {error && (
          <div className="alert" role="alert">
            {error}
          </div>
        )}
        <label>
          Email
          <input
            type="email"
            autoComplete="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            required
          />
        </label>
        <label>
          Password
          <input
            type="password"
            minLength={12}
            autoComplete={registering ? "new-password" : "current-password"}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            required
          />
        </label>
        <button className="primary-button" disabled={submitting}>
          {registering ? <UserPlus size={17} /> : <LogIn size={17} />}
          {submitting
            ? "Working..."
            : registering
              ? "Create account"
              : "Sign in"}
        </button>
        {authFeatures.oauth && !registering && (
          <button
            type="button"
            className="secondary-button"
            onClick={() => authApi.startOidc()}
          >
            Continue with SSO
          </button>
        )}
        <div className="auth-links">
          {authFeatures.passwordReset && (
            <Link to="/forgot-password">Forgot password?</Link>
          )}
          {authFeatures.registration && (
            <Link to={registering ? "/login" : "/register"}>
              {registering ? "Sign in" : "Create account"}
            </Link>
          )}
        </div>
      </form>
    </AuthFrame>
  );
}

export function ForgotPasswordPage() {
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState("");
  if (!authFeatures.passwordReset) return <Navigate to="/login" replace />;
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    try {
      await authApi.requestPasswordReset(email);
      setSent(true);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The request could not be completed",
      );
    }
  }
  return (
    <AuthFrame title="Reset password">
      {sent ? (
        <div className="auth-success">
          <Mail size={20} /> Check your email
        </div>
      ) : (
        <form className="auth-form" onSubmit={(event) => void submit(event)}>
          {error && (
            <div className="alert" role="alert">
              {error}
            </div>
          )}
          <label>
            Email
            <input
              type="email"
              autoComplete="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              required
            />
          </label>
          <button className="primary-button">
            <Mail size={17} /> Send reset link
          </button>
          <div className="auth-links">
            <Link to="/login">Back to sign in</Link>
          </div>
        </form>
      )}
    </AuthFrame>
  );
}

export function ResetPasswordPage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const token = params.get("token") ?? "";
  if (!authFeatures.passwordReset) return <Navigate to="/login" replace />;
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    try {
      await authApi.resetPassword(token, password);
      navigate("/login", { replace: true });
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The request could not be completed",
      );
    }
  }
  return (
    <AuthFrame title="Choose a password">
      <form className="auth-form" onSubmit={(event) => void submit(event)}>
        {error && (
          <div className="alert" role="alert">
            {error}
          </div>
        )}
        <label>
          New password
          <input
            type="password"
            minLength={12}
            autoComplete="new-password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            required
          />
        </label>
        <button className="primary-button" disabled={!token}>
          <KeyRound size={17} /> Update password
        </button>
      </form>
    </AuthFrame>
  );
}

export function VerifyEmailPage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const [state, setState] = useState<"pending" | "success" | "error">(
    "pending",
  );
  const [message, setMessage] = useState("");
  const token = params.get("token") ?? "";
  const requestedToken = useRef<string | null>(null);
  useEffect(() => {
    if (!token) {
      setState("error");
      setMessage("This verification link is missing its token.");
      return;
    }
    if (requestedToken.current === token) return;
    requestedToken.current = token;
    let active = true;
    authApi
      .verifyEmail(token)
      .then(() => {
        if (active) {
          setState("success");
          setMessage("Your email address is verified.");
        }
      })
      .catch((reason) => {
        if (active) setState("error");
        if (active)
          setMessage(
            reason instanceof Error
              ? reason.message
              : "The verification link is invalid or expired",
          );
      });
    return () => {
      active = false;
    };
  }, [token]);
  return (
    <AuthFrame
      title={
        state === "success"
          ? "Email verified"
          : state === "error"
            ? "Verification unavailable"
            : "Verifying email"
      }
    >
      {state === "pending" ? (
        <div className="auth-loading" aria-label="Loading" />
      ) : state === "success" ? (
        <>
          <div className="auth-success">
            <Mail size={20} /> {message}
          </div>
          <button
            type="button"
            className="primary-button"
            onClick={() => navigate("/", { replace: true })}
          >
            Continue
          </button>
        </>
      ) : (
        <div className="alert" role="alert">
          {message}
        </div>
      )}
    </AuthFrame>
  );
}

export function ApiTokensPage() {
  const queryClient = useQueryClient();
  const [created, setCreated] = useState<CreatedApiToken | null>(null);
  const [name, setName] = useState("");
  const [expires, setExpires] = useState("");
  const tokensQuery = useQuery({
    queryKey: appQueryKeys.tokens,
    queryFn: ({ signal }) => authApi.listApiTokens({ signal }),
  });
  const tokens = tokensQuery.data ?? [];
  const createMutation = useMutation({
    mutationFn: ({
      tokenName,
      expiresInDays,
    }: {
      tokenName: string;
      expiresInDays?: number;
    }) => authApi.createApiToken(tokenName, expiresInDays),
    onSuccess: (token) => {
      setCreated(token);
      queryClient.setQueryData(
        appQueryKeys.tokens,
        (items: ApiToken[] = []) => [token, ...items],
      );
      setName("");
      setExpires("");
    },
  });
  const revokeMutation = useMutation({
    mutationFn: authApi.revokeApiToken,
    onSuccess: (_, id) => {
      queryClient.setQueryData(appQueryKeys.tokens, (items: ApiToken[] = []) =>
        items.map((token) =>
          token.id === id
            ? { ...token, revoked_at: new Date().toISOString() }
            : token,
        ),
      );
    },
  });
  const requestError =
    createMutation.error ?? revokeMutation.error ?? tokensQuery.error;
  const error = requestError ? errorMessage(requestError) : "";

  async function create(event: FormEvent) {
    event.preventDefault();
    try {
      await createMutation.mutateAsync({
        tokenName: name,
        expiresInDays: expires ? Number(expires) : undefined,
      });
    } catch {
      // The mutation state renders the request error.
    }
  }
  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <h1>API tokens</h1>
          <p>Use personal tokens for scripts and automation.</p>
        </div>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      {created && (
        <section className="alert" role="status">
          <strong>Copy this token now. It will not be shown again.</strong>
          <div className="toolbar">
            <code>{created.token}</code>
            <button
              type="button"
              className="icon-button"
              title="Copy token"
              aria-label="Copy token"
              onClick={() => void navigator.clipboard.writeText(created.token)}
            >
              <Copy size={15} />
            </button>
          </div>
        </section>
      )}
      <section className="form-frame token-form">
        <form className="toolbar" onSubmit={(event) => void create(event)}>
          <label className="sr-only" htmlFor="token-name">
            Token name
          </label>
          <input
            id="token-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Token name"
            maxLength={80}
            required
          />
          <label className="sr-only" htmlFor="token-expiry">
            Expires in days
          </label>
          <input
            id="token-expiry"
            type="number"
            min={1}
            max={3650}
            value={expires}
            onChange={(event) => setExpires(event.target.value)}
            placeholder="Days (optional)"
          />
          <button
            className="primary-button"
            disabled={createMutation.isPending}
          >
            <Plus size={16} /> Create token
          </button>
        </form>
      </section>
      <section className="table-frame token-list">
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Created</th>
              <th>Expires</th>
              <th>Status</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {tokens.map((token) => (
              <tr key={token.id}>
                <td>{token.name}</td>
                <td>{new Date(token.created_at).toLocaleDateString()}</td>
                <td>
                  {token.expires_at
                    ? new Date(token.expires_at).toLocaleDateString()
                    : "Never"}
                </td>
                <td>{token.revoked_at ? "Revoked" : "Active"}</td>
                <td>
                  {!token.revoked_at && (
                    <button
                      type="button"
                      className="icon-button danger"
                      title="Revoke token"
                      aria-label={`Revoke ${token.name}`}
                      disabled={revokeMutation.isPending}
                      onClick={() => revokeMutation.mutate(token.id)}
                    >
                      <Trash2 size={15} />
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {tokens.length === 0 && <div className="empty">No API tokens yet</div>}
      </section>
    </main>
  );
}

export function AdminPage() {
  const auth = useAuth();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const overviewQuery = useQuery({
    queryKey: appQueryKeys.admin.overview,
    queryFn: ({ signal }) => adminApi.overview({ signal }),
    enabled: isAdmin,
  });
  if (!isAdmin) return <Navigate to="/" replace />;
  const overview: AdminOverview | null = overviewQuery.data ?? null;
  const error = overviewQuery.error ? errorMessage(overviewQuery.error) : "";
  const metrics = overview
    ? ([
        ["Users", overview.users],
        ["Organizations", overview.organizations],
        ["Invitations", overview.invitations],
        ["Sessions", overview.sessions],
        ["Jobs queued", overview.jobs_queued],
        ["Jobs dead", overview.jobs_dead],
        ["Mail deliveries", overview.mail_deliveries],
        ["Files", overview.files],
        ["Audit events", overview.audit_events],
      ] as const)
    : [];
  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <h1>Administration</h1>
          <p>Operational status across generated modules.</p>
        </div>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      {overview ? (
        <div className="admin-grid">
          {metrics.map(([label, value]) => (
            <section className="admin-metric" key={label}>
              <span>{label}</span>
              <strong>{value.toLocaleString()}</strong>
            </section>
          ))}
        </div>
      ) : (
        !error && <div className="auth-loading" aria-label="Loading" />
      )}
      <nav className="admin-links" aria-label="Administration pages">
        <Link to="/admin/users">Users</Link>
        <Link to="/tokens">API tokens</Link>
        {adminFeatures.jobs && <Link to="/admin/jobs">Jobs</Link>}
        {adminFeatures.jobs && <Link to="/admin/schedules">Schedules</Link>}
        {adminFeatures.webhooks && (
          <Link to="/admin/webhooks">Webhook deliveries</Link>
        )}
        {adminFeatures.mail && <Link to="/admin/mail">Mail deliveries</Link>}
        {adminFeatures.file && <Link to="/admin/files">Files</Link>}
        {adminFeatures.tenant && <Link to="/organization">Organization</Link>}
        {adminFeatures.audit && <Link to="/audit">Audit log</Link>}
      </nav>
    </main>
  );
}

export function AdminUsersPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const [page, setPage] = useState(1);
  const pageSize = 25;
  const queryKey = appQueryKeys.admin.users(page, pageSize);
  const usersQuery = useQuery({
    queryKey,
    queryFn: ({ signal }) =>
      adminApi.listUsers({ page, page_size: pageSize }, { signal }),
    enabled: isAdmin,
    placeholderData: (previous) => previous,
  });
  const revokeMutation = useMutation({
    mutationFn: adminApi.revokeUserSessions,
    onSuccess: (_, id) => {
      queryClient.setQueryData(queryKey, (response: typeof usersQuery.data) =>
        response
          ? {
              ...response,
              data: response.data.map((user) =>
                user.id === id ? { ...user, active_sessions: 0 } : user,
              ),
            }
          : response,
      );
    },
  });
  if (!isAdmin) return <Navigate to="/admin" replace />;
  const users = usersQuery.data?.data ?? [];
  const total = usersQuery.data?.meta.total ?? 0;
  const requestError = revokeMutation.error ?? usersQuery.error;
  const error = requestError ? errorMessage(requestError) : "";

  async function revokeSessions(user: AdminUser) {
    try {
      await revokeMutation.mutateAsync(user.id);
    } catch {
      // The mutation state renders the request error.
    }
  }
  return (
    <main className="page">
      <Link className="back-link" to="/admin">
        <ArrowLeft size={15} /> Administration
      </Link>
      <div className="page-heading">
        <div>
          <h1>Users</h1>
          <p>Registered accounts and active sessions.</p>
        </div>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      <section className="table-frame admin-users-table">
        <table>
          <thead>
            <tr>
              <th>Email</th>
              <th>Roles</th>
              <th>Verified</th>
              <th>Active sessions</th>
              <th>Created</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <tr key={user.id}>
                <td>{user.email}</td>
                <td>{user.roles.join(", ")}</td>
                <td>{user.email_verified ? "Yes" : "No"}</td>
                <td>{user.active_sessions}</td>
                <td>{new Date(user.created_at).toLocaleString()}</td>
                <td>
                  <button
                    type="button"
                    className="icon-button"
                    title="Revoke all sessions"
                    aria-label={`Revoke all sessions for ${user.email}`}
                    disabled={revokeMutation.isPending}
                    onClick={() => void revokeSessions(user)}
                  >
                    <LogIn size={15} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {users.length === 0 && !error && (
          <div className="empty">No users yet</div>
        )}
      </section>
      <AdminPagination
        page={page}
        pageSize={pageSize}
        total={total}
        onPageChange={setPage}
      />
    </main>
  );
}

export function AdminJobsPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const [status, setStatus] = useState<"" | AdminJobStatus>("");
  const [page, setPage] = useState(1);
  const pageSize = 25;
  const queryKey = appQueryKeys.admin.jobs(status, page, pageSize);
  const jobsQuery = useQuery({
    queryKey,
    queryFn: ({ signal }) =>
      adminApi.listJobs(
        { status: status || undefined, page, page_size: pageSize },
        { signal },
      ),
    enabled: adminFeatures.jobs && isAdmin,
    placeholderData: (previous) => previous,
  });
  const jobMutation = useMutation({
    mutationFn: ({
      id,
      operation,
    }: {
      id: string;
      operation: "retry" | "replay";
    }) =>
      operation === "retry" ? adminApi.retryJob(id) : adminApi.replayJob(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: appQueryKeys.admin.all });
    },
  });
  if (!isAdmin || !adminFeatures.jobs) return <Navigate to="/admin" replace />;
  const jobs: AdminJob[] = jobsQuery.data?.data ?? [];
  const total = jobsQuery.data?.meta.total ?? 0;
  const requestError = jobMutation.error ?? jobsQuery.error;
  const error = requestError ? errorMessage(requestError) : "";
  const busy = jobMutation.isPending;

  async function mutate(id: string, operation: "retry" | "replay") {
    try {
      await jobMutation.mutateAsync({ id, operation });
    } catch {
      // The mutation state renders the request error.
    }
  }
  return (
    <main className="page">
      <Link className="back-link" to="/admin">
        <ArrowLeft size={15} /> Administration
      </Link>
      <div className="page-heading">
        <div>
          <h1>Jobs</h1>
          <p>Inspect recent work and recover terminal jobs.</p>
        </div>
        <label className="filter-control">
          Status
          <select
            value={status}
            onChange={(event) => {
              setStatus(event.target.value as "" | AdminJobStatus);
              setPage(1);
            }}
          >
            <option value="">All statuses</option>
            <option value="queued">Queued</option>
            <option value="running">Running</option>
            <option value="succeeded">Succeeded</option>
            <option value="dead">Dead</option>
          </select>
        </label>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      <section className="table-frame admin-jobs-table">
        <table>
          <thead>
            <tr>
              <th>Job</th>
              <th>Queue</th>
              <th>Status</th>
              <th>Attempts</th>
              <th>Run at</th>
              <th>Last error</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {jobs.map((job) => (
              <tr key={job.id}>
                <td title={job.id}>
                  <strong>{job.kind}</strong>
                </td>
                <td>{job.queue}</td>
                <td>
                  <span className={`job-status ${job.status}`}>
                    {job.status}
                  </span>
                </td>
                <td>
                  {job.attempts} / {job.max_attempts}
                </td>
                <td>{new Date(job.run_at).toLocaleString()}</td>
                <td title={job.last_error ?? undefined}>
                  {job.last_error ?? "-"}
                </td>
                <td>
                  <div className="row-actions">
                    {job.status === "dead" && (
                      <button
                        type="button"
                        className="icon-button"
                        title="Retry job"
                        aria-label={`Retry ${job.kind}`}
                        disabled={busy}
                        onClick={() => void mutate(job.id, "retry")}
                      >
                        <RotateCcw size={15} />
                      </button>
                    )}
                    {(job.status === "succeeded" || job.status === "dead") && (
                      <button
                        type="button"
                        className="icon-button"
                        title="Replay as a new job"
                        aria-label={`Replay ${job.kind}`}
                        disabled={busy}
                        onClick={() => void mutate(job.id, "replay")}
                      >
                        <CopyPlus size={15} />
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {jobs.length === 0 && (
          <div className="empty">No jobs match this status</div>
        )}
      </section>
      <AdminPagination
        page={page}
        pageSize={pageSize}
        total={total}
        onPageChange={setPage}
      />
    </main>
  );
}

export function AdminWebhooksPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const [status, setStatus] = useState<"" | AdminWebhookStatus>("");
  const [page, setPage] = useState(1);
  const pageSize = 25;
  const queryKey = appQueryKeys.admin.webhooks(status, page, pageSize);
  const webhooksQuery = useQuery({
    queryKey,
    queryFn: ({ signal }) =>
      adminApi.listWebhooks(
        { status: status || undefined, page, page_size: pageSize },
        { signal },
      ),
    enabled: adminFeatures.webhooks && isAdmin,
    placeholderData: (previous) => previous,
  });
  const webhookMutation = useMutation({
    mutationFn: ({
      id,
      operation,
    }: {
      id: string;
      operation: "retry" | "replay";
    }) =>
      operation === "retry"
        ? adminApi.retryWebhook(id)
        : adminApi.replayWebhook(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: appQueryKeys.admin.all });
    },
  });
  if (!isAdmin || !adminFeatures.webhooks)
    return <Navigate to="/admin" replace />;
  const deliveries: AdminWebhookDelivery[] = webhooksQuery.data?.data ?? [];
  const total = webhooksQuery.data?.meta.total ?? 0;
  const requestError = webhookMutation.error ?? webhooksQuery.error;
  const error = requestError ? errorMessage(requestError) : "";
  const busy = webhookMutation.isPending;

  async function mutate(id: string, operation: "retry" | "replay") {
    try {
      await webhookMutation.mutateAsync({ id, operation });
    } catch {
      // The mutation state renders the request error.
    }
  }
  return (
    <main className="page">
      <Link className="back-link" to="/admin">
        <ArrowLeft size={15} /> Administration
      </Link>
      <div className="page-heading">
        <div>
          <h1>Webhook deliveries</h1>
          <p>
            Inspect downstream delivery failures and replay terminal events.
          </p>
        </div>
        <label className="filter-control">
          Status
          <select
            value={status}
            onChange={(event) => {
              setStatus(event.target.value as "" | AdminWebhookStatus);
              setPage(1);
            }}
          >
            <option value="">All statuses</option>
            <option value="pending">Pending</option>
            <option value="delivering">Delivering</option>
            <option value="succeeded">Succeeded</option>
            <option value="dead">Dead</option>
          </select>
        </label>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      <section className="table-frame admin-webhooks-table">
        <table>
          <thead>
            <tr>
              <th>Event</th>
              <th>Endpoint</th>
              <th>Status</th>
              <th>Attempts</th>
              <th>HTTP</th>
              <th>Last error</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {deliveries.map((delivery) => (
              <tr key={delivery.id}>
                <td title={delivery.id}>
                  <strong>{delivery.event}</strong>
                </td>
                <td>{delivery.endpoint}</td>
                <td>
                  <span className={`job-status ${delivery.status}`}>
                    {delivery.status}
                  </span>
                </td>
                <td>
                  {delivery.attempts} / {delivery.max_attempts}
                </td>
                <td>{delivery.response_status ?? "-"}</td>
                <td title={delivery.last_error ?? undefined}>
                  {delivery.last_error ?? "-"}
                </td>
                <td>
                  <div className="row-actions">
                    {delivery.status === "dead" && (
                      <button
                        type="button"
                        className="icon-button"
                        title="Retry delivery"
                        aria-label={`Retry ${delivery.event}`}
                        disabled={busy}
                        onClick={() => void mutate(delivery.id, "retry")}
                      >
                        <RotateCcw size={15} />
                      </button>
                    )}
                    {(delivery.status === "succeeded" ||
                      delivery.status === "dead") && (
                      <button
                        type="button"
                        className="icon-button"
                        title="Replay as a new delivery"
                        aria-label={`Replay ${delivery.event}`}
                        disabled={busy}
                        onClick={() => void mutate(delivery.id, "replay")}
                      >
                        <CopyPlus size={15} />
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {deliveries.length === 0 && (
          <div className="empty">No webhook deliveries match this status</div>
        )}
      </section>
      <AdminPagination
        page={page}
        pageSize={pageSize}
        total={total}
        onPageChange={setPage}
      />
    </main>
  );
}

export function AdminPagination({
  page,
  pageSize,
  total,
  onPageChange,
}: {
  page: number;
  pageSize: number;
  total: number;
  onPageChange(page: number): void;
}) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  return (
    <div className="pagination">
      <span>
        Page {page} of {pages} ({total} total)
      </span>
      <div>
        <button
          type="button"
          className="icon-button"
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
          aria-label="Previous page"
        >
          <ChevronLeft size={17} />
        </button>
        <button
          type="button"
          className="icon-button"
          disabled={page >= pages}
          onClick={() => onPageChange(page + 1)}
          aria-label="Next page"
        >
          <ChevronRight size={17} />
        </button>
      </div>
    </div>
  );
}

function AuthFrame({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <main className="auth-page">
      <section className="auth-panel">
        <div className="auth-brand">__APP_TITLE__</div>
        <h1>{title}</h1>
        {children}
      </section>
    </main>
  );
}
