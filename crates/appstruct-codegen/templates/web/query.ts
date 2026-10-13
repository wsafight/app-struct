import { QueryClient } from "@tanstack/react-query";
import { ApiError } from "./generated/client";

export function shouldRetryQuery(
  failureCount: number,
  error: unknown,
): boolean {
  if (error instanceof ApiError)
    return (error.status === 429 || error.status >= 500) && failureCount < 2;
  return error instanceof TypeError && failureCount < 2;
}

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      gcTime: 5 * 60_000,
      retry: shouldRetryQuery,
      refetchOnWindowFocus: false,
      refetchOnReconnect: true,
    },
    mutations: {
      retry: 0,
    },
  },
});

/**
 * Keep tenant-scoped server state in a distinct cache namespace. The Tenant
 * provider still resets active queries during a switch so the UI clears
 * immediately, while this namespace prevents a late response from the old
 * organization from populating the cache read by the new organization.
 */
export function currentTenantScope(): string {
  try {
    return window.localStorage.getItem("appstruct_tenant") ?? "global";
  } catch {
    return "global";
  }
}

export const resourceQueryKeys = {
  all: (resourceId: string) =>
    ["resource", currentTenantScope(), resourceId] as const,
  lists: (resourceId: string) =>
    [...resourceQueryKeys.all(resourceId), "list"] as const,
  list: (resourceId: string, query: string) =>
    [...resourceQueryKeys.lists(resourceId), query] as const,
  details: (resourceId: string) =>
    [...resourceQueryKeys.all(resourceId), "detail"] as const,
  detail: (resourceId: string, id: string) =>
    [...resourceQueryKeys.details(resourceId), id] as const,
  options: (resourceId: string, query = "") =>
    [...resourceQueryKeys.all(resourceId), "options", query] as const,
  aggregate: (resourceId: string, query: string) =>
    [...resourceQueryKeys.all(resourceId), "aggregate", query] as const,
};

export const appQueryKeys = {
  session: ["session"] as const,
  tenant: {
    all: ["tenant"] as const,
    organizations: ["tenant", "organizations"] as const,
    invitations: (organizationId: string) =>
      ["tenant", organizationId, "invitations"] as const,
  },
  audit: (page: number, pageSize: number, entity = "", recordId = "") =>
    [
      "audit",
      currentTenantScope(),
      { page, pageSize, entity, recordId },
    ] as const,
  activity: (resource: string, recordId: string) =>
    ["activity", currentTenantScope(), resource, recordId] as const,
  reports: {
    all: () => ["reports", currentTenantScope()] as const,
    templates: () => ["reports", currentTenantScope(), "templates"] as const,
    runs: (page: number, pageSize: number) =>
      ["reports", currentTenantScope(), "runs", { page, pageSize }] as const,
  },
  tokens: ["auth", "tokens"] as const,
  admin: {
    all: ["admin"] as const,
    overview: ["admin", "overview"] as const,
    users: (page: number, pageSize: number) =>
      ["admin", "users", { page, pageSize }] as const,
    jobs: (status: string, page: number, pageSize: number) =>
      ["admin", "jobs", { status, page, pageSize }] as const,
    schedules: ["admin", "schedules"] as const,
    webhooks: (status: string, page: number, pageSize: number) =>
      ["admin", "webhooks", { status, page, pageSize }] as const,
    mail: (search: string, page: number, pageSize: number) =>
      ["admin", "mail", { search, page, pageSize }] as const,
    mailDetail: (id: string) => ["admin", "mail", id] as const,
    files: (search: string, page: number, pageSize: number) =>
      ["admin", "files", { search, page, pageSize }] as const,
    fileDetail: (id: string) => ["admin", "files", id] as const,
    billingEvents: (page: number, pageSize: number) =>
      ["admin", "billing", "events", { page, pageSize }] as const,
  },
};
