pub(super) fn tenant_storage_source() -> &'static str {
    r#"const TENANT_STORAGE_KEY = "appstruct_tenant";

function currentTenant(): string | undefined {
  return window.localStorage.getItem(TENANT_STORAGE_KEY) ?? undefined;
}

function selectTenant(id?: string): void {
  if (id) window.localStorage.setItem(TENANT_STORAGE_KEY, id);
  else window.localStorage.removeItem(TENANT_STORAGE_KEY);
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

export const tenantApi = {
  listOrganizations: () => request<{ data: TenantOrganization[] }>("/api/tenant/organizations"),
  createOrganization: (name: string) => request<TenantOrganization>("/api/tenant/organizations", {
    method: "POST",
    body: JSON.stringify({ name }),
  }),
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
  list: (query: Pick<ListQuery, "page" | "page_size"> = {}) =>
    request<ListResponse<AuditEvent>>(listPath("/api/audit/events", query)),
};
"#
    .to_owned()
}
