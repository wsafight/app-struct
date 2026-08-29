use appstruct_ir::AppIr;

pub(super) fn source(ir: &AppIr) -> String {
    let registration = ir.auth.registration_enabled;
    let password_reset = ir.auth.password_reset_enabled;
    let oauth = ir.auth.oauth_enabled;
    let tenant = ir.tenant.enabled;
    let audit = ir.audit.enabled;
    let jobs = ir.jobs.enabled;
    format!(
        r#"export interface AuthUser {{
  id: string;
  email: string;
  roles: string[];
}}

interface AuthResponse {{ user: AuthUser; email_verified: boolean; }}

export interface ApiToken {{
  id: string;
  name: string;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
  revoked_at: string | null;
}}

export interface CreatedApiToken extends ApiToken {{ token: string; }}

export interface AdminOverview {{
  users: number;
  organizations: number;
  invitations: number;
  sessions: number;
  jobs_queued: number;
  jobs_dead: number;
  mail_deliveries: number;
  files: number;
  audit_events: number;
}}

export interface AdminUser {{
  id: string;
  email: string;
  roles: string[];
  email_verified: boolean;
  active_sessions: number;
  created_at: string;
}}

export interface AdminSessionRevocation {{ revoked: number; }}

export type AdminJobStatus = "queued" | "running" | "succeeded" | "dead";

export interface AdminJob {{
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
}}

export const authFeatures = {{ registration: {registration}, passwordReset: {password_reset}, emailVerification: true, oauth: {oauth} }} as const;
export const adminFeatures = {{ tenant: {tenant}, audit: {audit}, jobs: {jobs} }} as const;

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
  listApiTokens: () => request<ApiToken[]>("/api/auth/tokens"),
  createApiToken: (name: string, expiresInDays?: number) => request<CreatedApiToken>("/api/auth/tokens", {{ method: "POST", body: JSON.stringify({{ name, expires_in_days: expiresInDays }}) }}),
  revokeApiToken: (id: string) => request<void>(`/api/auth/tokens/${{id}}`, {{ method: "DELETE" }}),
}};

export const adminApi = {{
  overview: () => request<AdminOverview>("/api/admin/overview"),
  listUsers: (limit = 50) =>
    request<{{ data: AdminUser[] }}>(`/api/admin/users?limit=${{limit}}`).then((response) => response.data),
  revokeUserSessions: (id: string) =>
    request<AdminSessionRevocation>(`/api/admin/users/${{id}}/revoke-sessions`, {{ method: "POST" }}),
  listJobs: (status?: AdminJobStatus) =>
    request<{{ data: AdminJob[] }}>(`/api/admin/jobs${{status ? `?status=${{status}}` : ""}}`).then((response) => response.data),
  retryJob: (id: string) => request<AdminJob>(`/api/admin/jobs/${{id}}/retry`, {{ method: "POST" }}),
  replayJob: (id: string) => request<AdminJob>(`/api/admin/jobs/${{id}}/replay`, {{ method: "POST" }}),
}};
"#
    )
}
