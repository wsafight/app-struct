import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { ListQuery } from "./generated/client";
import { resourceQueryKeys } from "./query";
import type { ResourceDefinition, ResourceRecord } from "./resource";
import { useCanAccess } from "./resource";

export interface ResourceListControllerOptions {
  cacheKey: string;
  query: ListQuery;
  trashMode?: boolean;
  enabled?: boolean;
  onChangeSuccess?(): void;
}

const EMPTY_LIST = { data: [] as ResourceRecord[], total: 0 };

export function useResourceListController(resource: ResourceDefinition, options: ResourceListControllerOptions) {
  const canList = useCanAccess(resource, "list");
  const queryClient = useQueryClient();
  const listQuery = useQuery({
    queryKey: resourceQueryKeys.list(resource.id, `${options.trashMode ? "trash" : "active"}:${options.cacheKey}`),
    queryFn: async ({ signal }) => {
      if (options.trashMode) {
        const response = await resource.api.trash?.({ page: options.query.page, page_size: options.query.page_size }, { signal });
        return { data: response?.data ?? [], total: response?.meta.total ?? 0 };
      }
      const response = await resource.api.list(options.query, { signal });
      return { data: response.data, total: response.meta.total };
    },
    enabled: canList && (options.enabled ?? true),
    placeholderData: (previous) => previous,
  });
  const change = useMutation({
    mutationFn: (operation: () => Promise<void>) => operation(),
    onSuccess: async () => {
      options.onChangeSuccess?.();
      await queryClient.invalidateQueries({ queryKey: resourceQueryKeys.all(resource.id) });
    },
  });

  async function runChange(operation: () => Promise<void>): Promise<boolean> {
    try {
      await change.mutateAsync(operation);
      return true;
    } catch {
      return false;
    }
  }

  const result = listQuery.data ?? EMPTY_LIST;
  return {
    canList,
    records: result.data,
    total: result.total,
    pending: listQuery.isPending,
    fetching: listQuery.isFetching,
    changing: change.isPending,
    dataUpdatedAt: listQuery.dataUpdatedAt,
    error: change.error ?? listQuery.error,
    refetch: listQuery.refetch,
    runChange,
  };
}

export function useResourceDetailController(resource: ResourceDefinition, id?: string) {
  const canRead = useCanAccess(resource, "read");
  const query = useQuery({
    queryKey: resourceQueryKeys.detail(resource.id, id ?? ""),
    queryFn: ({ signal }) => resource.api.get(id!, { signal }),
    enabled: Boolean(id && canRead),
  });
  const canUpdate = useCanAccess(resource, "update", query.data);
  return {
    canRead,
    canUpdate,
    record: query.data,
    pending: query.isPending,
    fetching: query.isFetching,
    error: query.error,
    refetch: query.refetch,
  };
}
