import { useMemo } from "react";
import { useSearchParams, validateResourceSearch } from "./navigation";
import {
  canAccessRule,
  useResourceActor,
  type FieldDefinition,
  type ResourceDefinition,
} from "./resource";

export function parseResourceQuery(
  resource: ResourceDefinition,
  fields: FieldDefinition[],
  parameters: URLSearchParams,
) {
  const search = validateResourceSearch(Object.fromEntries(parameters));
  const pageSize = search.page_size ?? 25;
  const sort = search.sort ?? "";
  const trashMode = resource.softDelete && search.trash === "1";
  const cursorMode = !trashMode && !sort;
  const cursor = cursorMode ? search.cursor : undefined;
  const page = cursorMode && !cursor ? 1 : (search.page ?? 1);
  const filters = buildResourceFilterQuery(fields, parameters);
  return {
    page,
    pageSize,
    sort,
    trashMode,
    cursorMode,
    query: cursorMode
      ? {
          cursor,
          direction: cursor ? (search.direction ?? "next") : undefined,
          limit: pageSize,
          q: search.q || undefined,
          ...filters,
        }
      : {
          page,
          page_size: pageSize,
          sort: sort || undefined,
          q: search.q || undefined,
          ...filters,
        },
  };
}

export function useResourceUrlController(resource: ResourceDefinition) {
  const actor = useResourceActor();
  const [searchParams, setSearchParams] = useSearchParams();
  const filterFields = useMemo(
    () =>
      resource.fields.filter(
        (field) =>
          field.filterable &&
          canAccessRule(field.readAccess ?? { mode: "public" }, actor),
      ),
    [actor, resource],
  );
  const state = parseResourceQuery(resource, filterFields, searchParams);
  function updateParam(name: string, value?: string, replace = false) {
    setSearchParams(
      (current) => {
        const next = new URLSearchParams(current);
        if (value) next.set(name, value);
        else next.delete(name);
        if (name !== "page") {
          next.delete("page");
          next.delete("cursor");
          next.delete("direction");
        }
        return next;
      },
      { replace },
    );
  }
  return {
    ...state,
    searchParams,
    setSearchParams,
    filterFields,
    updateParam,
    moveCursor(cursor: string, direction: "next" | "previous", page: number) {
      setSearchParams((current) => {
        const next = new URLSearchParams(current);
        next.set("cursor", cursor);
        if (direction === "previous") next.set("direction", direction);
        else next.delete("direction");
        if (page > 1) next.set("page", String(page));
        else next.delete("page");
        return next;
      });
    },
    queryString: searchParams.toString(),
  };
}

export function buildResourceFilterQuery(
  fields: FieldDefinition[],
  searchParams: URLSearchParams,
) {
  return {
    filters: Object.fromEntries(
      fields.map((field) => [
        field.name,
        searchParams.get(`filter[${field.name}]`) ?? "",
      ]),
    ),
    range_filters: Object.fromEntries(
      fields.filter(supportsRange).map((field) => [
        field.name,
        {
          gte: searchParams.get(`filter[${field.name}][gte]`) ?? "",
          lte: searchParams.get(`filter[${field.name}][lte]`) ?? "",
        },
      ]),
    ),
  };
}

export function supportsRange(field: FieldDefinition): boolean {
  return ["integer", "bigint", "decimal", "date", "datetime"].includes(
    field.kind,
  );
}
