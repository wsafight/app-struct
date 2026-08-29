import { FormEvent, ReactNode, useEffect, useState } from "react";
import { ArrowLeft, Copy, CopyPlus, KeyRound, LogIn, Mail, Plus, RotateCcw, Trash2, UserPlus } from "lucide-react";
import { Link, Navigate, useLocation, useNavigate, useSearchParams } from "../navigation";
import { adminApi, adminFeatures, authApi, authFeatures, type AdminJob, type AdminJobStatus, type AdminOverview, type ApiToken, type CreatedApiToken } from "../generated/client";
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
  if (auth.user) return <Navigate to="/" replace />;

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSubmitting(true);
    setError("");
    try {
      if (mode === "login") await auth.login(email, password); else await auth.register(email, password);
      const from = (location.state as { from?: { pathname?: string } } | null)?.from?.pathname ?? "/";
      navigate(from, { replace: true });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "The request could not be completed");
    } finally {
      setSubmitting(false);
    }
  }

  const registering = mode === "register";
  return <AuthFrame title={registering ? "Create account" : "Sign in"}>
    <form className="auth-form" onSubmit={(event) => void submit(event)}>
      {error && <div className="alert" role="alert">{error}</div>}
      <label>Email<input type="email" autoComplete="email" value={email} onChange={(event) => setEmail(event.target.value)} required /></label>
      <label>Password<input type="password" minLength={12} autoComplete={registering ? "new-password" : "current-password"} value={password} onChange={(event) => setPassword(event.target.value)} required /></label>
      <button className="primary-button" disabled={submitting}>{registering ? <UserPlus size={17} /> : <LogIn size={17} />}{submitting ? "Working..." : registering ? "Create account" : "Sign in"}</button>
      {authFeatures.oauth && !registering && <button type="button" className="secondary-button" onClick={() => authApi.startOidc()}>Continue with SSO</button>}
      <div className="auth-links">
        {authFeatures.passwordReset && <Link to="/forgot-password">Forgot password?</Link>}
        {authFeatures.registration && <Link to={registering ? "/login" : "/register"}>{registering ? "Sign in" : "Create account"}</Link>}
      </div>
    </form>
  </AuthFrame>;
}

export function ForgotPasswordPage() {
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState("");
  if (!authFeatures.passwordReset) return <Navigate to="/login" replace />;
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    try { await authApi.requestPasswordReset(email); setSent(true); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "The request could not be completed"); }
  }
  return <AuthFrame title="Reset password">{sent ? <div className="auth-success"><Mail size={20} /> Check your email</div> : <form className="auth-form" onSubmit={(event) => void submit(event)}>{error && <div className="alert" role="alert">{error}</div>}<label>Email<input type="email" autoComplete="email" value={email} onChange={(event) => setEmail(event.target.value)} required /></label><button className="primary-button"><Mail size={17} /> Send reset link</button><div className="auth-links"><Link to="/login">Back to sign in</Link></div></form>}</AuthFrame>;
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
    try { await authApi.resetPassword(token, password); navigate("/login", { replace: true }); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "The request could not be completed"); }
  }
  return <AuthFrame title="Choose a password"><form className="auth-form" onSubmit={(event) => void submit(event)}>{error && <div className="alert" role="alert">{error}</div>}<label>New password<input type="password" minLength={12} autoComplete="new-password" value={password} onChange={(event) => setPassword(event.target.value)} required /></label><button className="primary-button" disabled={!token}><KeyRound size={17} /> Update password</button></form></AuthFrame>;
}

export function VerifyEmailPage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const [state, setState] = useState<"pending" | "success" | "error">("pending");
  const [message, setMessage] = useState("");
  const token = params.get("token") ?? "";
  useEffect(() => {
    if (!token) { setState("error"); setMessage("This verification link is missing its token."); return; }
    authApi.verifyEmail(token)
      .then(() => { setState("success"); setMessage("Your email address is verified."); })
      .catch((reason) => { setState("error"); setMessage(reason instanceof Error ? reason.message : "The verification link is invalid or expired"); });
  }, [token]);
  return <AuthFrame title={state === "success" ? "Email verified" : state === "error" ? "Verification unavailable" : "Verifying email"}>
    {state === "pending" ? <div className="auth-loading" aria-label="Loading" /> : state === "success" ? <><div className="auth-success"><Mail size={20} /> {message}</div><button type="button" className="primary-button" onClick={() => navigate("/", { replace: true })}>Continue</button></> : <div className="alert" role="alert">{message}</div>}
  </AuthFrame>;
}

export function ApiTokensPage() {
  const [tokens, setTokens] = useState<ApiToken[]>([]);
  const [created, setCreated] = useState<CreatedApiToken | null>(null);
  const [name, setName] = useState("");
  const [expires, setExpires] = useState("");
  const [error, setError] = useState("");
  useEffect(() => { authApi.listApiTokens().then(setTokens).catch((reason) => setError(reason instanceof Error ? reason.message : "Unable to load tokens")); }, []);
  async function create(event: FormEvent) {
    event.preventDefault(); setError("");
    try { const token = await authApi.createApiToken(name, expires ? Number(expires) : undefined); setCreated(token); setTokens((items) => [token, ...items]); setName(""); setExpires(""); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "Unable to create token"); }
  }
  async function revoke(id: string) {
    try { await authApi.revokeApiToken(id); setTokens((items) => items.map((token) => token.id === id ? { ...token, revoked_at: new Date().toISOString() } : token)); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "Unable to revoke token"); }
  }
  return <main className="page"><div className="page-heading"><div><h1>API tokens</h1><p>Use personal tokens for scripts and automation.</p></div></div>{error && <div className="alert" role="alert">{error}</div>}{created && <section className="alert" role="status"><strong>Copy this token now. It will not be shown again.</strong><div className="toolbar"><code>{created.token}</code><button type="button" className="icon-button" title="Copy token" aria-label="Copy token" onClick={() => void navigator.clipboard.writeText(created.token)}><Copy size={15} /></button></div></section>}<section className="form-frame token-form"><form className="toolbar" onSubmit={(event) => void create(event)}><label className="sr-only" htmlFor="token-name">Token name</label><input id="token-name" value={name} onChange={(event) => setName(event.target.value)} placeholder="Token name" maxLength={80} required /><label className="sr-only" htmlFor="token-expiry">Expires in days</label><input id="token-expiry" type="number" min={1} max={3650} value={expires} onChange={(event) => setExpires(event.target.value)} placeholder="Days (optional)" /><button className="primary-button"><Plus size={16} /> Create token</button></form></section><section className="table-frame token-list"><table><thead><tr><th>Name</th><th>Created</th><th>Expires</th><th>Status</th><th aria-label="Actions" /></tr></thead><tbody>{tokens.map((token) => <tr key={token.id}><td>{token.name}</td><td>{new Date(token.created_at).toLocaleDateString()}</td><td>{token.expires_at ? new Date(token.expires_at).toLocaleDateString() : "Never"}</td><td>{token.revoked_at ? "Revoked" : "Active"}</td><td>{!token.revoked_at && <button type="button" className="icon-button danger" title="Revoke token" aria-label={`Revoke ${token.name}`} onClick={() => void revoke(token.id)}><Trash2 size={15} /></button>}</td></tr>)}</tbody></table>{tokens.length === 0 && <div className="empty">No API tokens yet</div>}</section></main>;
}

export function AdminPage() {
  const auth = useAuth();
  const [overview, setOverview] = useState<AdminOverview | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!auth.user?.roles.includes("admin")) return;
    adminApi.overview().then(setOverview).catch((reason) => setError(reason instanceof Error ? reason.message : "Unable to load admin overview"));
  }, [auth.user]);
  if (!auth.user?.roles.includes("admin")) return <Navigate to="/" replace />;
  const metrics = overview ? [
    ["Users", overview.users], ["Organizations", overview.organizations], ["Invitations", overview.invitations], ["Sessions", overview.sessions],
    ["Jobs queued", overview.jobs_queued], ["Jobs dead", overview.jobs_dead], ["Mail deliveries", overview.mail_deliveries], ["Files", overview.files], ["Audit events", overview.audit_events],
  ] as const : [];
  return <main className="page"><div className="page-heading"><div><h1>Administration</h1><p>Operational status across generated modules.</p></div></div>{error && <div className="alert" role="alert">{error}</div>}{overview ? <div className="admin-grid">{metrics.map(([label, value]) => <section className="admin-metric" key={label}><span>{label}</span><strong>{value.toLocaleString()}</strong></section>)}</div> : !error && <div className="auth-loading" aria-label="Loading" />}<nav className="admin-links" aria-label="Administration pages"><Link to="/tokens">API tokens</Link>{adminFeatures.jobs && <Link to="/admin/jobs">Jobs</Link>}{adminFeatures.tenant && <Link to="/organization">Organization</Link>}{adminFeatures.audit && <Link to="/audit">Audit log</Link>}</nav></main>;
}

export function AdminJobsPage() {
  const auth = useAuth();
  const [jobs, setJobs] = useState<AdminJob[]>([]);
  const [status, setStatus] = useState<"" | AdminJobStatus>("");
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    if (!adminFeatures.jobs || !auth.user?.roles.includes("admin")) return;
    setError("");
    adminApi.listJobs(status || undefined).then(setJobs).catch((reason) => setError(reason instanceof Error ? reason.message : "Unable to load jobs"));
  }, [auth.user, status]);
  if (!auth.user?.roles.includes("admin") || !adminFeatures.jobs) return <Navigate to="/admin" replace />;
  async function mutate(id: string, operation: "retry" | "replay") {
    setBusy(`${operation}:${id}`);
    setError("");
    try {
      const job = operation === "retry" ? await adminApi.retryJob(id) : await adminApi.replayJob(id);
      setJobs((items) => operation === "retry" ? items.map((item) => item.id === id ? job : item) : [job, ...items]);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : `Unable to ${operation} job`);
    } finally {
      setBusy("");
    }
  }
  return <main className="page"><Link className="back-link" to="/admin"><ArrowLeft size={15} /> Administration</Link><div className="page-heading"><div><h1>Jobs</h1><p>Inspect recent work and recover terminal jobs.</p></div><label className="filter-control">Status<select value={status} onChange={(event) => setStatus(event.target.value as "" | AdminJobStatus)}><option value="">All statuses</option><option value="queued">Queued</option><option value="running">Running</option><option value="succeeded">Succeeded</option><option value="dead">Dead</option></select></label></div>{error && <div className="alert" role="alert">{error}</div>}<section className="table-frame admin-jobs-table"><table><thead><tr><th>Job</th><th>Queue</th><th>Status</th><th>Attempts</th><th>Run at</th><th>Last error</th><th aria-label="Actions" /></tr></thead><tbody>{jobs.map((job) => <tr key={job.id}><td title={job.id}><strong>{job.kind}</strong></td><td>{job.queue}</td><td><span className={`job-status ${job.status}`}>{job.status}</span></td><td>{job.attempts} / {job.max_attempts}</td><td>{new Date(job.run_at).toLocaleString()}</td><td title={job.last_error ?? undefined}>{job.last_error ?? "-"}</td><td><div className="row-actions">{job.status === "dead" && <button type="button" className="icon-button" title="Retry job" aria-label={`Retry ${job.kind}`} disabled={Boolean(busy)} onClick={() => void mutate(job.id, "retry")}><RotateCcw size={15} /></button>}{(job.status === "succeeded" || job.status === "dead") && <button type="button" className="icon-button" title="Replay as a new job" aria-label={`Replay ${job.kind}`} disabled={Boolean(busy)} onClick={() => void mutate(job.id, "replay")}><CopyPlus size={15} /></button>}</div></td></tr>)}</tbody></table>{jobs.length === 0 && <div className="empty">No jobs match this status</div>}</section></main>;
}

function AuthFrame({ title, children }: { title: string; children: ReactNode }) {
  return <main className="auth-page"><section className="auth-panel"><div className="auth-brand">AppStruct</div><h1>{title}</h1>{children}</section></main>;
}
