import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
      refetchOnWindowFocus: false,
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
