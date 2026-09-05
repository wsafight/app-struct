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
  const page = search.page ?? 1;
  const pageSize = search.page_size ?? 25;
  const sort = search.sort ?? "";
  return {
    page,
    pageSize,
    sort,
    trashMode: resource.softDelete && search.trash === "1",
    query: {
      page,
      page_size: pageSize,
      sort: sort || undefined,
      q: search.q || undefined,
      ...buildResourceFilterQuery(fields, parameters),
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
        if (name !== "page") next.delete("page");
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
