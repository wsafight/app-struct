import { QueryClient } from "@tanstack/react-query";
import { ApiError } from "./generated/client";

export function shouldRetryQuery(failureCount: number, error: unknown): boolean {
  if (error instanceof ApiError) return (error.status === 429 || error.status >= 500) && failureCount < 2;
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

export const resourceQueryKeys = {
  all: (resourceId: string) => ["resource", resourceId] as const,
  lists: (resourceId: string) => [...resourceQueryKeys.all(resourceId), "list"] as const,
  list: (resourceId: string, query: string) => [...resourceQueryKeys.lists(resourceId), query] as const,
  details: (resourceId: string) => [...resourceQueryKeys.all(resourceId), "detail"] as const,
  detail: (resourceId: string, id: string) => [...resourceQueryKeys.details(resourceId), id] as const,
  options: (resourceId: string, query = "") => [...resourceQueryKeys.all(resourceId), "options", query] as const,
};
