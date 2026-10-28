import { formatDate, formatDateTime, formatNumber } from "../ui";
import { t } from "../ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  Copy,
  CopyPlus,
  LogIn,
  Plus,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { Link, Navigate } from "../navigation";
import {
  adminApi,
  adminFeatures,
  authApi,
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
          <h1> {t("API tokens")} </h1>
          <p> {t("Use personal tokens for scripts and automation.")}</p>
        </div>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      {created && (
        <section className="alert" role="status">
          <strong>
            {" "}
            {t("Copy this token now. It will not be shown again.")}
          </strong>
          <div className="toolbar">
            <code>{created.token}</code>
            <button
              type="button"
              className="icon-button"
              title={t("Copy token")}
              aria-label={t("Copy token")}
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
            {t("Token name")}
          </label>
          <input
            id="token-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={t("Token name")}
            maxLength={80}
            required
          />
          <label className="sr-only" htmlFor="token-expiry">
            {" "}
            {t("Expires in days")}
          </label>
          <input
            id="token-expiry"
            type="number"
            min={1}
            max={3650}
            value={expires}
            onChange={(event) => setExpires(event.target.value)}
            placeholder={t("Days (optional)")}
          />
          <button
            className="primary-button"
            disabled={createMutation.isPending}
          >
            <Plus size={16} /> {t("Create token")}
          </button>
        </form>
      </section>
      <section className="table-frame token-list">
        <table>
          <thead>
            <tr>
              <th>{t("Name")}</th>
              <th>{t("Created")}</th>
              <th> {t("Expires")}</th>
              <th> {t("Status")}</th>
              <th aria-label={t("Actions")} />
            </tr>
          </thead>
          <tbody>
            {tokens.map((token) => (
              <tr key={token.id}>
                <td>{token.name}</td>
                <td>{formatDate(token.created_at)}</td>
                <td>
                  {token.expires_at ? formatDate(token.expires_at) : t("Never")}
                </td>
                <td>{token.revoked_at ? t("Revoked") : t("Active")}</td>
                <td>
                  {!token.revoked_at && (
                    <button
                      type="button"
                      className="icon-button danger"
                      title={t("Revoke token")}
                      aria-label={t("Revoke {0}", { "0": token.name })}
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
        {tokens.length === 0 && (
          <div className="empty"> {t("No API tokens yet")}</div>
        )}
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
        [t("Users"), overview.users],
        [t("Organizations"), overview.organizations],
        [t("Invitations"), overview.invitations],
        [t("Sessions"), overview.sessions],
        [t("Jobs queued"), overview.jobs_queued],
        [t("Jobs dead"), overview.jobs_dead],
        [t("Mail deliveries"), overview.mail_deliveries],
        [t("Files"), overview.files],
        [t("Audit events"), overview.audit_events],
        [t("Billing events"), overview.billing_events],
      ] as const)
    : [];
  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <h1>{t("Administration")}</h1>
          <p>{t("Operational status across generated modules.")}</p>
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
              <strong>{formatNumber(value)}</strong>
            </section>
          ))}
        </div>
      ) : (
        !error && <div className="auth-loading" aria-label={t("Loading")} />
      )}
      <nav className="admin-links" aria-label={t("Administration pages")}>
        <Link to="/admin/users"> {t("Users")}</Link>
        <Link to="/tokens"> {t("API tokens")} </Link>
        {adminFeatures.jobs && <Link to="/admin/jobs"> {t("Jobs")}</Link>}
        {adminFeatures.jobs && (
          <Link to="/admin/schedules">{t("Schedules")}</Link>
        )}
        {adminFeatures.webhooks && (
          <Link to="/admin/webhooks"> {t("Webhook deliveries")}</Link>
        )}
        {adminFeatures.mail && (
          <Link to="/admin/mail">{t("Mail deliveries")}</Link>
        )}
        {adminFeatures.file && <Link to="/admin/files">{t("Files")}</Link>}
        {adminFeatures.tenant && (
          <Link to="/organization">{t("Organization")}</Link>
        )}
        {adminFeatures.audit && <Link to="/audit">{t("Audit log")}</Link>}
        {adminFeatures.billing && (
          <Link to="/admin/billing">{t("Billing events")}</Link>
        )}
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
        <ArrowLeft size={15} /> {t("Administration")}
      </Link>
      <div className="page-heading">
        <div>
          <h1> {t("Users")}</h1>
          <p>{t("Registered accounts and active sessions.")}</p>
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
              <th> {t("Email")}</th>
              <th> {t("Roles")}</th>
              <th> {t("Verified")} </th>
              <th>{t("Active sessions")}</th>
              <th>{t("Created")}</th>
              <th aria-label={t("Actions")} />
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <tr key={user.id}>
                <td>{user.email}</td>
                <td>{user.roles.join(", ")}</td>
                <td>{user.email_verified ? t("Yes") : t("No")}</td>
                <td>{user.active_sessions}</td>
                <td>{formatDateTime(user.created_at)}</td>
                <td>
                  <button
                    type="button"
                    className="icon-button"
                    title={t("Revoke all sessions")}
                    aria-label={t("Revoke all sessions for {0}", {
                      "0": user.email,
                    })}
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
          <div className="empty">{t("No users yet")}</div>
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
        <ArrowLeft size={15} /> {t("Administration")}
      </Link>
      <div className="page-heading">
        <div>
          <h1> {t("Jobs")}</h1>
          <p>{t("Inspect recent work and recover terminal jobs.")}</p>
        </div>
        <label className="filter-control">
          {" "}
          {t("Status")}
          <select
            value={status}
            onChange={(event) => {
              setStatus(event.target.value as "" | AdminJobStatus);
              setPage(1);
            }}
          >
            <option value="">{t("All statuses")}</option>
            <option value="queued"> {t("Queued")}</option>
            <option value="running">{t("Running")}</option>
            <option value="succeeded"> {t("Succeeded")}</option>
            <option value="dead"> {t("Dead")}</option>
            <option value="cancelled">{t("Cancelled")}</option>
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
              <th> {t("Job")} </th>
              <th>{t("Queue")}</th>
              <th> {t("Status")}</th>
              <th>{t("Attempts")}</th>
              <th> {t("Run at")} </th>
              <th>{t("Last error")}</th>
              <th aria-label={t("Actions")} />
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
                    {t(job.status)}
                  </span>
                </td>
                <td>
                  {job.attempts} / {job.max_attempts}
                </td>
                <td>{formatDateTime(job.run_at)}</td>
                <td title={job.last_error ?? undefined}>
                  {job.last_error ?? "-"}
                </td>
                <td>
                  <div className="row-actions">
                    {job.status === "dead" && (
                      <button
                        type="button"
                        className="icon-button"
                        title={t("Retry job")}
                        aria-label={t("Retry {0}", { "0": job.kind })}
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
                        title={t("Replay as a new job")}
                        aria-label={t("Replay {0}", { "0": job.kind })}
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
          <div className="empty">{t("No jobs match this status")}</div>
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
        <ArrowLeft size={15} /> {t("Administration")}
      </Link>
      <div className="page-heading">
        <div>
          <h1> {t("Webhook deliveries")}</h1>
          <p>
            {" "}
            {t(
              "Inspect downstream delivery failures and replay terminal events.",
            )}
          </p>
        </div>
        <label className="filter-control">
          {" "}
          {t("Status")}
          <select
            value={status}
            onChange={(event) => {
              setStatus(event.target.value as "" | AdminWebhookStatus);
              setPage(1);
            }}
          >
            <option value="">{t("All statuses")}</option>
            <option value="pending">{t("Pending")}</option>
            <option value="delivering">{t("Delivering")}</option>
            <option value="succeeded"> {t("Succeeded")}</option>
            <option value="dead"> {t("Dead")}</option>
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
              <th>{t("Event")}</th>
              <th>{t("Endpoint")}</th>
              <th> {t("Status")}</th>
              <th>{t("Attempts")}</th>
              <th>HTTP</th>
              <th>{t("Last error")}</th>
              <th aria-label={t("Actions")} />
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
                    {t(delivery.status)}
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
                        title={t("Retry delivery")}
                        aria-label={t("Retry {0}", { "0": delivery.event })}
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
                        title={t("Replay as a new delivery")}
                        aria-label={t("Replay {0}", { "0": delivery.event })}
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
          <div className="empty">
            {t("No webhook deliveries match this status")}
          </div>
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
        {t("Page {0} of {1} ({2} total)", { 0: page, 1: pages, 2: total })}
      </span>
      <div>
        <button
          type="button"
          className="icon-button"
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
          aria-label={t("Previous page")}
        >
          <ChevronLeft size={17} />
        </button>
        <button
          type="button"
          className="icon-button"
          disabled={page >= pages}
          onClick={() => onPageChange(page + 1)}
          aria-label={t("Next page")}
        >
          <ChevronRight size={17} />
        </button>
      </div>
    </div>
  );
}
