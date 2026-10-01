import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { resourceQueryKeys } from "./query";
import type { ResourceDefinition, ResourceRecord } from "./resource";
import { canAccessResource, errorMessage, useResourceActor } from "./resource";

export const RELATION_OPTION_PAGE_SIZE = 25;

export interface RelationOptions {
  options: ResourceRecord[];
  pages: number;
  loadError: string;
  canLoad: boolean;
  isFetching: boolean;
  isPending: boolean;
  search: string;
  page: number;
  setSearch(value: string): void;
  setPage(value: number): void;
}

/**
 * Load the selectable records for a relation field, plus the record a form or filter
 * currently points at.
 *
 * Both the form's relation picker and the list's relation filter need the same paged,
 * searchable, access-gated lookup; keeping it in one place means the two cannot drift
 * on pagination, deduplication or the selected-record merge.
 */
export function useRelationOptions(
  target: ResourceDefinition | undefined,
  value: string,
): RelationOptions {
  const actor = useResourceActor();
  const canLoad = Boolean(target && canAccessResource(target, "list", actor));
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const deferredSearch = useDebouncedValue(search, 250);
  const optionsQuery = useQuery({
    queryKey: resourceQueryKeys.options(
      target?.id ?? "unavailable",
      `${deferredSearch}:${page}`,
    ),
    queryFn: ({ signal }) =>
      target!.api.list(
        {
          page,
          page_size: RELATION_OPTION_PAGE_SIZE,
          q: deferredSearch || undefined,
        },
        { signal },
      ),
    enabled: canLoad,
    placeholderData: (previous) => previous,
  });
  const selectedQuery = useQuery({
    queryKey: resourceQueryKeys.detail(target?.id ?? "unavailable", value),
    queryFn: ({ signal }) => target!.api.get(value, { signal }),
    enabled: canLoad && Boolean(value),
  });
  const primaryKey = target?.primaryKey ?? "id";
  const options = [
    ...(selectedQuery.data && value ? [selectedQuery.data] : []),
    ...(optionsQuery.data?.data ?? []),
  ].filter(
    (record, index, items) =>
      String(record[primaryKey]) &&
      items.findIndex(
        (candidate) =>
          String(candidate[primaryKey]) === String(record[primaryKey]),
      ) === index,
  );
  return {
    options,
    pages: Math.max(
      1,
      Math.ceil(
        (optionsQuery.data?.meta.total ?? 0) / RELATION_OPTION_PAGE_SIZE,
      ),
    ),
    loadError: optionsQuery.error ? errorMessage(optionsQuery.error) : "",
    canLoad,
    isFetching: optionsQuery.isFetching,
    isPending: optionsQuery.isPending,
    search,
    page,
    setSearch,
    setPage,
  };
}

export function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delay);
    return () => window.clearTimeout(timer);
  }, [delay, value]);
  return debounced;
}
