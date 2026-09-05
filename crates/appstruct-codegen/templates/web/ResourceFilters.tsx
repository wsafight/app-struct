import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { type InputHTMLAttributes, useEffect, useState } from "react";
import {
  inputType,
  toApiValue,
  toFormValue,
  valueError,
} from "../field-values";
import { supportsRange } from "../url-controller";
export { buildResourceFilterQuery, supportsRange } from "../url-controller";
import { resourceQueryKeys } from "../query";
import { recordLabel } from "../relations";
import type { FieldDefinition, ResourceDefinition } from "../resource";
import { canAccessResource, errorMessage, useResourceActor } from "../resource";

interface ResourceFiltersProps {
  fields: FieldDefinition[];
  resources: ResourceDefinition[];
  searchParams: URLSearchParams;
  updateParam(name: string, value?: string, replace?: boolean): void;
}

export function ResourceFilters({
  fields,
  resources,
  searchParams,
  updateParam,
}: ResourceFiltersProps) {
  return fields.map((field) => (
    <FilterControl
      key={field.name}
      field={field}
      resources={resources}
      searchParams={searchParams}
      updateParam={updateParam}
    />
  ));
}

function FilterControl({
  field,
  resources,
  searchParams,
  updateParam,
}: Omit<ResourceFiltersProps, "fields"> & { field: FieldDefinition }) {
  if (supportsRange(field)) {
    return (
      <label className="filter-control">
        <span>{field.label}</span>
        <span className="range-filter">
          {(["gte", "lte"] as const).map((operator) => {
            const name = `filter[${field.name}][${operator}]`;
            return (
              <DebouncedFilterInput
                key={operator}
                type={inputType(field.kind)}
                aria-label={`${field.label} ${operator === "gte" ? "from" : "to"}`}
                placeholder={operator === "gte" ? "From" : "To"}
                value={filterDisplayValue(
                  searchParams.get(name) ?? "",
                  field.kind,
                )}
                onValueChange={(value) =>
                  updateParam(name, filterApiValue(value, field.kind), true)
                }
                step={
                  field.kind === "datetime" || field.kind === "decimal"
                    ? "any"
                    : undefined
                }
              />
            );
          })}
        </span>
      </label>
    );
  }
  if (field.kind === "relation") {
    return (
      <RelationFilter
        field={field}
        target={resources.find((resource) => resource.id === field.relation)}
        value={searchParams.get(`filter[${field.name}]`) ?? ""}
        onChange={(value) => updateParam(`filter[${field.name}]`, value, true)}
      />
    );
  }
  const name = `filter[${field.name}]`;
  if (field.kind === "enum" || field.kind === "boolean") {
    const values =
      field.kind === "boolean" ? ["true", "false"] : (field.values ?? []);
    return (
      <label className="filter-control">
        <span>{field.label}</span>
        <select
          value={searchParams.get(name) ?? ""}
          onChange={(event) =>
            updateParam(name, event.target.value || undefined, true)
          }
        >
          <option value="">All</option>
          {values.map((value) => (
            <option key={value} value={value}>
              {field.kind === "boolean"
                ? value === "true"
                  ? "Yes"
                  : "No"
                : value}
            </option>
          ))}
        </select>
      </label>
    );
  }
  return (
    <label className="filter-control">
      <span>{field.label}</span>
      <DebouncedFilterInput
        value={searchParams.get(name) ?? ""}
        onValueChange={(value) => updateParam(name, value || undefined, true)}
      />
    </label>
  );
}

function DebouncedFilterInput({
  value,
  onValueChange,
  ...props
}: { value: string; onValueChange(value: string): void } & Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "value" | "onChange"
>) {
  const [draft, setDraft] = useState(value);
  useEffect(() => {
    setDraft(value);
  }, [value]);
  useEffect(() => {
    if (draft === value) return;
    const timer = window.setTimeout(() => onValueChange(draft), 300);
    return () => window.clearTimeout(timer);
  }, [draft, onValueChange, value]);
  return (
    <input
      {...props}
      value={draft}
      onChange={(event) => setDraft(event.target.value)}
    />
  );
}

function RelationFilter({
  field,
  target,
  value,
  onChange,
}: {
  field: FieldDefinition;
  target?: ResourceDefinition;
  value: string;
  onChange(value?: string): void;
}) {
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
        { page, page_size: 25, q: deferredSearch || undefined },
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
  const loadError = optionsQuery.error ? errorMessage(optionsQuery.error) : "";
  const pages = Math.max(
    1,
    Math.ceil((optionsQuery.data?.meta.total ?? 0) / 25),
  );
  const options = [
    ...(selectedQuery.data && value ? [selectedQuery.data] : []),
    ...(optionsQuery.data?.data ?? []),
  ].filter(
    (record, index, items) =>
      String(record[target?.primaryKey ?? "id"]) &&
      items.findIndex(
        (candidate) =>
          String(candidate[target?.primaryKey ?? "id"]) ===
          String(record[target?.primaryKey ?? "id"]),
      ) === index,
  );
  return (
    <div className="filter-control">
      <span>{field.label}</span>
      <input
        value={search}
        placeholder="Search"
        aria-label={`Search ${field.label}`}
        onChange={(event) => {
          setSearch(event.target.value);
          setPage(1);
        }}
      />
      <select
        value={value}
        aria-label={field.label}
        aria-busy={optionsQuery.isFetching}
        aria-invalid={Boolean(loadError)}
        disabled={optionsQuery.isPending && canLoad}
        onChange={(event) => onChange(event.target.value || undefined)}
      >
        <option value="">All</option>
        {options.map((record) => {
          const optionValue = String(record[target?.primaryKey ?? "id"]);
          return (
            <option key={optionValue} value={optionValue}>
              {target ? recordLabel(target, record) : String(record.id)}
            </option>
          );
        })}
      </select>
      <span className="relation-pages">
        <button
          type="button"
          className="icon-button"
          disabled={page <= 1}
          onClick={() => setPage((current) => current - 1)}
          aria-label="Previous options"
        >
          <ChevronLeft size={14} />
        </button>
        <span aria-live="polite">
          {page} / {pages}
        </span>
        <button
          type="button"
          className="icon-button"
          disabled={page >= pages}
          onClick={() => setPage((current) => current + 1)}
          aria-label="Next options"
        >
          <ChevronRight size={14} />
        </button>
      </span>
      <span className="sr-only" role="status" aria-live="polite">
        {optionsQuery.isFetching
          ? `Loading ${field.label} options`
          : loadError || `${options.length} ${field.label} options loaded`}
      </span>
    </div>
  );
}

function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delay);
    return () => window.clearTimeout(timer);
  }, [delay, value]);
  return debounced;
}

function filterDisplayValue(
  value: string,
  kind: FieldDefinition["kind"],
): string {
  return String(
    toFormValue(value || undefined, {
      name: "filter",
      label: "Filter",
      kind,
      required: false,
    }),
  );
}

function filterApiValue(
  value: string,
  kind: FieldDefinition["kind"],
): string | undefined {
  if (!value) return undefined;
  const field = { name: "filter", label: "Filter", kind, required: false };
  return valueError(value, field)
    ? undefined
    : String(toApiValue(value, field));
}
