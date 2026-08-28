pub(super) fn runtime_source() -> &'static str {
    r#"async function requestText(path: string, init?: RequestInit): Promise<string> {
  const headers = new Headers(init?.headers);
  const method = init?.method ?? "GET";
  if (method !== "GET" && method !== "HEAD") {
    const csrf = cookieValue("appstruct_csrf");
    if (csrf) headers.set("X-CSRF-Token", csrf);
  }
  const tenant = currentTenant();
  if (tenant) headers.set("X-AppStruct-Tenant", tenant);
  const response = await fetch(`${API_BASE}${path}`, { ...init, headers, credentials: "include" });
  if (!response.ok) throw new ApiError(response.status, "HTTP_ERROR", `Request failed with status ${response.status}`);
  return response.text();
}

export interface BulkFailure { id: string; code: string; message: string; }
export interface BulkResult { succeeded: string[]; failed: BulkFailure[]; }
export interface BulkUpdateRequest<T> { ids: string[]; patch: T; expected_revisions: Record<string, number>; }
export interface BulkDeleteRequest { ids: string[]; expected_revisions: Record<string, number>; }
"#
}
