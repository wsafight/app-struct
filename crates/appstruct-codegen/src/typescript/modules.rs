pub(super) fn tenant_storage_source() -> &'static str {
    r#"export const tenantStorageKey = "appstruct_tenant";

function browserStorage(): Storage | undefined {
  try {
    const storage = window.localStorage;
    return typeof storage?.getItem === "function" &&
      typeof storage.setItem === "function" &&
      typeof storage.removeItem === "function"
      ? storage
      : undefined;
  } catch {
    return undefined;
  }
}

function currentTenant(): string | undefined {
  try {
    return browserStorage()?.getItem(tenantStorageKey) ?? undefined;
  } catch {
    return undefined;
  }
}

function selectTenant(id?: string): void {
  try {
    const storage = browserStorage();
    if (id) storage?.setItem(tenantStorageKey, id);
    else storage?.removeItem(tenantStorageKey);
  } catch {
    // Storage can reject writes in privacy modes or restricted frames.
  }
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
  listOrganizations: (options: RequestOptions = {}) => request<{ data: TenantOrganization[] }>("/api/tenant/organizations", options),
  createOrganization: (name: string) => request<TenantOrganization>("/api/tenant/organizations", {
    method: "POST",
    body: JSON.stringify({ name }),
  }),
  listInvitations: (options: RequestOptions = {}) => request<{ data: TenantInvitation[] }>("/api/tenant/invitations", options),
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
  operation: "create" | "update" | "delete" | "restore";
  actor_id: string | null;
  tenant_id: string | null;
  before: unknown | null;
  after: unknown | null;
  occurred_at: string;
}

export interface AuditListQuery extends Pick<ListQuery, "page" | "page_size"> {
  entity?: string;
  record_id?: string;
}

export const auditApi = {
  list: (query: AuditListQuery = {}, options: RequestOptions = {}) => {
    const params = new URLSearchParams();
    if (query.page) params.set("page", String(query.page));
    if (query.page_size) params.set("page_size", String(query.page_size));
    if (query.entity) params.set("entity", query.entity);
    if (query.record_id) params.set("record_id", query.record_id);
    const search = params.toString();
    return request<ListResponse<AuditEvent>>(
      search ? `/api/audit/events?${search}` : "/api/audit/events",
      options,
    );
  },
};
"#
    .to_owned()
}
