use appstruct_ir::AppIr;

pub(super) fn source(ir: &AppIr) -> String {
    let mut source = types_source().to_owned();
    source.push_str(&api_source(ir));
    source
}

fn types_source() -> &'static str {
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

function adminListPath(
  path: string,
  query: AdminListQuery & { status?: string },
): string {
  const params = new URLSearchParams();
  if (query.page) params.set("page", String(query.page));
  if (query.page_size) params.set("page_size", String(query.page_size));
  if (query.status) params.set("status", query.status);
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
    format!(
        r#"export const authFeatures = {{ registration: {registration}, passwordReset: {password_reset}, emailVerification: true, oauth: {oauth} }} as const;
export const adminFeatures = {{ tenant: {tenant}, audit: {audit}, jobs: {jobs}, webhooks: {webhooks} }} as const;

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
  listWebhooks: (query: AdminListQuery & {{ status?: AdminWebhookStatus }} = {{}}, options: RequestOptions = {{}}) =>
    request<AdminListResponse<AdminWebhookDelivery>>(adminListPath("/api/admin/webhooks", query), options),
  retryWebhook: (id: string) => request<AdminWebhookDelivery>(`/api/admin/webhooks/${{id}}/retry`, {{ method: "POST" }}),
  replayWebhook: (id: string) => request<AdminWebhookDelivery>(`/api/admin/webhooks/${{id}}/replay`, {{ method: "POST" }}),
}};
"#
    )
}
