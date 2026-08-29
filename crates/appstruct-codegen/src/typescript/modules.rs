pub(super) fn tenant_storage_source() -> &'static str {
    r#"export const tenantStorageKey = "appstruct_tenant";

function currentTenant(): string | undefined {
  return window.localStorage.getItem(tenantStorageKey) ?? undefined;
}

function selectTenant(id?: string): void {
  if (id) window.localStorage.setItem(tenantStorageKey, id);
  else window.localStorage.removeItem(tenantStorageKey);
  resourceEtags.clear();
}
"#
}

pub(super) fn tenant_source() -> String {
    r#"export interface TenantOrganization {
  id: string;
  name: string;
  role: "owner" | "member";
  created_at: string;
}

export interface TenantInvitation {
  id: string;
  email: string;
  role: "member";
  expires_at: string;
  accepted_at: string | null;
  created_at: string;
}

export const tenantApi = {
  listOrganizations: () => request<{ data: TenantOrganization[] }>("/api/tenant/organizations"),
  createOrganization: (name: string) => request<TenantOrganization>("/api/tenant/organizations", {
    method: "POST",
    body: JSON.stringify({ name }),
  }),
  listInvitations: () => request<{ data: TenantInvitation[] }>("/api/tenant/invitations"),
  invite: (email: string, role: TenantInvitation["role"] = "member") =>
    request<TenantInvitation>("/api/tenant/invitations", {
      method: "POST", body: JSON.stringify({ email, role }),
    }),
  revokeInvitation: (id: string) => request<void>(`/api/tenant/invitations/${id}`, { method: "DELETE" }),
  acceptInvitation: (token: string) =>
    request<TenantOrganization>(`/api/tenant/invitations/${encodeURIComponent(token)}/accept`, { method: "POST" }),
  select: (id: string) => selectTenant(id),
  clear: () => selectTenant(),
  current: () => currentTenant(),
};
"#
    .to_owned()
}

pub(super) fn audit_source() -> String {
    r#"export interface AuditEvent {
  id: string;
  entity: string;
  record_id: string;
  operation: "create" | "update" | "delete";
  actor_id: string | null;
  tenant_id: string | null;
  before: unknown | null;
  after: unknown | null;
  occurred_at: string;
}

export const auditApi = {
  list: (query: Pick<ListQuery, "page" | "page_size"> = {}, options: RequestOptions = {}) =>
    request<ListResponse<AuditEvent>>(listPath("/api/audit/events", query), options),
};
"#
    .to_owned()
}
