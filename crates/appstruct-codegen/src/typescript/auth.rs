use appstruct_ir::AppIr;

pub(super) fn source(ir: &AppIr) -> String {
    let mut source = types_source();
    source.push_str(&api_source(ir));
    source
}

fn types_source() -> String {
    [auth_types_source(), admin_types_source()].concat()
}

fn auth_types_source() -> &'static str {
    r#"export interface AuthUser {
  id: string;
  email: string;
  roles: string[];
}

interface AuthResponse { user: AuthUser; email_verified: boolean; }

export interface ApiToken {
  id: string;
  name: string;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
  revoked_at: string | null;
}

export interface CreatedApiToken extends ApiToken { token: string; }

export interface AdminOverview {
  users: number;
  organizations: number;
  invitations: number;
  sessions: number;
  jobs_queued: number;
  jobs_dead: number;
  mail_deliveries: number;
  files: number;
  audit_events: number;
}

export interface AdminUser {
  id: string;
  email: string;
  roles: string[];
  email_verified: boolean;
  active_sessions: number;
  created_at: string;
}

export interface AdminSessionRevocation { revoked: number; }

export interface AdminListQuery {
  page?: number;
  page_size?: number;
  search?: string;
}

export interface AdminListResponse<T> {
  data: T[];
  meta: { page: number; page_size: number; total: number };
}

export type AdminJobStatus = "queued" | "running" | "succeeded" | "dead";

export interface AdminJob {
  id: string;
  queue: string;
  kind: string;
  status: AdminJobStatus;
  tenant_id: string | null;
  attempts: number;
  max_attempts: number;
  run_at: string;
  last_error: string | null;
  created_at: string;
  completed_at: string | null;
}
"#
}

fn admin_types_source() -> &'static str {
    r#"
export interface AdminSchedule {
  id: string;
  name: string;
  cron: string;
  interval_seconds: number | null;
  queue: string;
  kind: string;
  enabled: boolean;
  paused: boolean;
  next_run_at: string;
  last_run_at: string | null;
  created_at: string;
}

export interface AdminScheduleTrigger { job_id: string; }

export type AdminWebhookStatus = "pending" | "delivering" | "succeeded" | "dead";

export interface AdminWebhookDelivery {
  id: string;
  endpoint: string;
  event: string;
  status: AdminWebhookStatus;
  tenant_id: string | null;
  attempts: number;
  max_attempts: number;
  next_attempt_at: string;
  response_status: number | null;
  last_error: string | null;
  created_at: string;
  completed_at: string | null;
}

export interface AdminMailSummary {
  id: string;
  provider: string;
  template: string;
  sender: string;
  recipient: string;
  subject: string;
  tenant_id: string | null;
  created_at: string;
}

export interface AdminMailDelivery extends AdminMailSummary {
  text_body: string;
  html_body: string | null;
}

export interface AdminFile {
  id: string;
  object_key: string;
  original_name: string;
  content_type: string;
  size: number;
  checksum: string;
  tenant_id: string | null;
  created_at: string;
}

export interface AdminFileListResponse extends AdminListResponse<AdminFile> {
  total_bytes: number;
}

function adminListPath(
  path: string,
  query: AdminListQuery & { status?: string },
): string {
  const params = new URLSearchParams();
  if (query.page) params.set("page", String(query.page));
  if (query.page_size) params.set("page_size", String(query.page_size));
  if (query.status) params.set("status", query.status);
  if (query.search) params.set("search", query.search);
  const search = params.toString();
  return search ? `${path}?${search}` : path;
}

"#
}

fn api_source(ir: &AppIr) -> String {
    let registration = ir.auth.registration_enabled;
    let password_reset = ir.auth.password_reset_enabled;
    let oauth = ir.auth.oauth_enabled;
    let tenant = ir.tenant.enabled;
    let audit = ir.audit.enabled;
    let jobs = ir.jobs.enabled;
    let webhooks = ir.webhooks.enabled;
    let mail = ir.mail.enabled;
    let file = ir.file.enabled;
    format!(
        r#"export const authFeatures = {{ registration: {registration}, passwordReset: {password_reset}, emailVerification: true, oauth: {oauth} }} as const;
export const adminFeatures = {{ tenant: {tenant}, audit: {audit}, jobs: {jobs}, webhooks: {webhooks}, mail: {mail}, file: {file} }} as const;

export const authApi = {{
  me: async (options: RequestOptions = {{}}) => (await request<AuthResponse>("/api/auth/me", options)).user,
  login: async (email: string, password: string) => {{
    const user = (await request<AuthResponse>("/api/auth/login", {{ method: "POST", body: JSON.stringify({{ email, password }}) }})).user;
    broadcastSessionChange();
    return user;
  }},
  register: async (email: string, password: string) => {{
    const user = (await request<AuthResponse>("/api/auth/register", {{ method: "POST", body: JSON.stringify({{ email, password }}) }})).user;
    broadcastSessionChange();
    return user;
  }},
  logout: async () => {{
    await request<void>("/api/auth/logout", {{ method: "POST" }});
    resourceEtags.clear();
    selectTenant();
    broadcastSessionChange();
  }},
  requestPasswordReset: (email: string) =>
    request<void>("/api/auth/password/request", {{ method: "POST", body: JSON.stringify({{ email }}) }}),
  resetPassword: (token: string, password: string) =>
    request<void>("/api/auth/password/reset", {{ method: "POST", body: JSON.stringify({{ token, password }}) }}),
  requestEmailVerification: () => request<void>("/api/auth/email/request", {{ method: "POST" }}),
  verifyEmail: (token: string) => request<void>("/api/auth/email/verify", {{ method: "POST", body: JSON.stringify({{ token }}) }}),
  startOidc: () => {{ window.location.assign(`${{API_BASE}}/api/auth/oauth/oidc/start`); }},
  listApiTokens: (options: RequestOptions = {{}}) => request<ApiToken[]>("/api/auth/tokens", options),
  createApiToken: (name: string, expiresInDays?: number) => request<CreatedApiToken>("/api/auth/tokens", {{ method: "POST", body: JSON.stringify({{ name, expires_in_days: expiresInDays }}) }}),
  revokeApiToken: (id: string) => request<void>(`/api/auth/tokens/${{id}}`, {{ method: "DELETE" }}),
}};

export const adminApi = {{
  overview: (options: RequestOptions = {{}}) => request<AdminOverview>("/api/admin/overview", options),
  listUsers: (query: AdminListQuery = {{}}, options: RequestOptions = {{}}) =>
    request<AdminListResponse<AdminUser>>(adminListPath("/api/admin/users", query), options),
  revokeUserSessions: (id: string) =>
    request<AdminSessionRevocation>(`/api/admin/users/${{id}}/revoke-sessions`, {{ method: "POST" }}),
  listJobs: (query: AdminListQuery & {{ status?: AdminJobStatus }} = {{}}, options: RequestOptions = {{}}) =>
    request<AdminListResponse<AdminJob>>(adminListPath("/api/admin/jobs", query), options),
  retryJob: (id: string) => request<AdminJob>(`/api/admin/jobs/${{id}}/retry`, {{ method: "POST" }}),
  replayJob: (id: string) => request<AdminJob>(`/api/admin/jobs/${{id}}/replay`, {{ method: "POST" }}),
  listSchedules: (options: RequestOptions = {{}}) =>
    request<{{ data: AdminSchedule[] }}>("/api/admin/schedules", options),
  pauseSchedule: (id: string) =>
    request<AdminSchedule>(`/api/admin/schedules/${{encodeURIComponent(id)}}/pause`, {{ method: "POST" }}),
  resumeSchedule: (id: string) =>
    request<AdminSchedule>(`/api/admin/schedules/${{encodeURIComponent(id)}}/resume`, {{ method: "POST" }}),
  triggerSchedule: (id: string) =>
    request<AdminScheduleTrigger>(`/api/admin/schedules/${{encodeURIComponent(id)}}/trigger`, {{ method: "POST" }}),
  listWebhooks: (query: AdminListQuery & {{ status?: AdminWebhookStatus }} = {{}}, options: RequestOptions = {{}}) =>
    request<AdminListResponse<AdminWebhookDelivery>>(adminListPath("/api/admin/webhooks", query), options),
  retryWebhook: (id: string) => request<AdminWebhookDelivery>(`/api/admin/webhooks/${{id}}/retry`, {{ method: "POST" }}),
  replayWebhook: (id: string) => request<AdminWebhookDelivery>(`/api/admin/webhooks/${{id}}/replay`, {{ method: "POST" }}),
  listMail: (query: AdminListQuery = {{}}, options: RequestOptions = {{}}) =>
    request<AdminListResponse<AdminMailSummary>>(adminListPath("/api/admin/mail", query), options),
  getMail: (id: string, options: RequestOptions = {{}}) =>
    request<AdminMailDelivery>(`/api/admin/mail/${{encodeURIComponent(id)}}`, options),
  listFiles: (query: AdminListQuery = {{}}, options: RequestOptions = {{}}) =>
    request<AdminFileListResponse>(adminListPath("/api/admin/files", query), options),
  getFile: (id: string, options: RequestOptions = {{}}) =>
    request<AdminFile>(`/api/admin/files/${{encodeURIComponent(id)}}`, options),
}};
"#
    )
}
