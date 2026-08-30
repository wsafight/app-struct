pub(super) fn source() -> &'static str {
    r#"export interface PresenceEntry {
  connection_id: string;
  actor_id: string;
  tenant_id: string | null;
  resource: string | null;
  record_id: string | null;
  connected_at: string;
  last_seen_at: string;
  expires_at: string;
}

export interface RealtimeScope { resource: string; recordId?: string; }
export interface RealtimeLockScope { resource: string; recordId: string; }
export interface RealtimeLockLease {
  lease_token: string;
  actor_id: string;
  tenant_id: string | null;
  resource: string;
  record_id: string;
  acquired_at: string;
  expires_at: string;
}

export function subscribeRealtime(scope: RealtimeScope): EventSource {
  const params = new URLSearchParams();
  const tenant = currentTenant();
  if (tenant) params.set("tenant_id", tenant);
  if (scope.resource) params.set("resource", scope.resource);
  if (scope.recordId) params.set("record_id", scope.recordId);
  const query = params.toString();
  return new EventSource(`${API_BASE}/api/realtime/events${query ? `?${query}` : ""}`, {
    withCredentials: true,
  });
}

export function listPresence(scope: RealtimeScope): Promise<PresenceEntry[]> {
  const params = new URLSearchParams();
  if (scope.resource) params.set("resource", scope.resource);
  if (scope.recordId) params.set("record_id", scope.recordId);
  const query = params.toString();
  return request<{ data: PresenceEntry[] }>(`/api/realtime/presence${query ? `?${query}` : ""}`)
    .then((response) => response.data);
}

function realtimeLockPath(scope: RealtimeLockScope, token?: string): string {
  const params = new URLSearchParams({ resource: scope.resource, record_id: scope.recordId });
  const suffix = token ? `/${encodeURIComponent(token)}` : "";
  return `/api/realtime/locks${suffix}?${params.toString()}`;
}

export function getRealtimeLock(scope: RealtimeLockScope): Promise<RealtimeLockLease | null> {
  return request<{ data: RealtimeLockLease | null }>(realtimeLockPath(scope))
    .then((response) => response.data);
}

export function acquireRealtimeLock(
  scope: RealtimeLockScope,
  ttlSeconds = 30,
): Promise<RealtimeLockLease> {
  return request<RealtimeLockLease>(realtimeLockPath(scope), {
    method: "POST", body: JSON.stringify({ ttl_seconds: ttlSeconds }),
  });
}

export function renewRealtimeLock(
  scope: RealtimeLockScope,
  token: string,
  ttlSeconds = 30,
): Promise<RealtimeLockLease> {
  return request<RealtimeLockLease>(realtimeLockPath(scope, token), {
    method: "PATCH", body: JSON.stringify({ ttl_seconds: ttlSeconds }),
  });
}

export function releaseRealtimeLock(scope: RealtimeLockScope, token: string): Promise<void> {
  return request<void>(realtimeLockPath(scope, token), { method: "DELETE" });
}
"#
}
