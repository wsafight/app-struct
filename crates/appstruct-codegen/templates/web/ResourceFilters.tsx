import { formatNumber, t } from "../ui";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { type InputHTMLAttributes, useEffect, useRef, useState } from "react";
import {
  inputType,
  toApiValue,
  toFormValue,
  valueError,
} from "../field-values";
import { supportsRange } from "../url-controller";
import { buildResourceFilterQuery } from "../url-controller";
import { resourceQueryKeys } from "../query";
import type { FilterExpression } from "../generated/client";
import { AdvancedFilters } from "./resource-list/AdvancedFilters";
import { parseEditableFilter } from "./resource-list/filter-expression";
export { filterOperators } from "./resource-list/filter-expression";
export { buildResourceFilterQuery, supportsRange } from "../url-controller";
import { recordLabel } from "../relations";
import { useRelationOptions } from "../relation-options";
import type { FieldDefinition, ResourceDefinition } from "../resource";

interface ResourceFiltersProps {
  resource?: ResourceDefinition;
  fields: FieldDefinition[];
  resources: ResourceDefinition[];
  searchParams: URLSearchParams;
  updateParam(name: string, value?: string, replace?: boolean): void;
  updateParams?(values: Record<string, string | undefined>): void;
}

export function ResourceFilters({
  resource,
  fields,
  resources,
  searchParams,
  updateParam,
  updateParams,
}: ResourceFiltersProps) {
  return (
    <>
      {fields.map((field) => (
        <FilterControl
          key={field.name}
          field={field}
          resources={resources}
          searchParams={searchParams}
          updateParam={updateParam}
        />
      ))}
      <AdvancedFilters
        fields={fields}
        raw={searchParams.get("where")}
        onApply={(value) => updateParam("where", value, false)}
      />
      {resource && (
        <FacetCounts
          resource={resource}
          fields={fields}
          parameters={searchParams}
          update={(values) => {
            if (updateParams) updateParams(values);
            else
              Object.entries(values).forEach(([name, value]) =>
                updateParam(name, value),
              );
          }}
        />
      )}
    </>
  );
}

function FacetCounts({
  resource,
  fields,
  parameters,
  update,
}: {
  resource: ResourceDefinition;
  fields: FieldDefinition[];
  parameters: URLSearchParams;
  update(values: Record<string, string | undefined>): void;
}) {
  const facets = fields
    .filter((field) => field.kind === "enum" || field.kind === "boolean")
    .slice(0, 5);
  const query = {
    ...buildResourceFilterQuery(fields, parameters),
    q: parameters.get("q") || undefined,
    facet: facets.map((field) => field.name),
    limit: 10,
  };
  const result = useQuery({
    queryKey: [...resourceQueryKeys.all(resource.id), "facets", query],
    queryFn: ({ signal }) => resource.api.facets!(query, { signal }),
    enabled: Boolean(resource.api.facets && facets.length),
    staleTime: 15_000,
  });
  if (!result.data)
    return result.isError ? (
      <p role="status">{t("Filter counts are unavailable.")}</p>
    ) : null;
  return (
    <div className="facet-counts" aria-label={t("Counts for current filters")}>
      {facets.map((field) => {
        const facet = result.data.data[field.name];
        return (
          facet && (
            <div key={field.name} className="facet-group">
              <strong>{field.label}:</strong>{" "}
              {facet.buckets.map((bucket, index) => {
                const expression = facetExpression(parameters.get("where"));
                const children =
                  expression && "all" in expression
                    ? expression.all
                    : expression
                      ? [expression]
                      : [];
                const condition: FilterExpression =
                  bucket.value === null
                    ? { field: field.name, op: "is_null" }
                    : {
                        field: field.name,
                        op: "eq",
                        values: [String(bucket.value)],
                      };
                const isFieldCondition = (child: FilterExpression) =>
                  "field" in child &&
                  child.field === field.name &&
                  ["eq", "is_null"].includes(child.op);
                const selected =
                  children.some(
                    (child) =>
                      isFieldCondition(child) &&
                      JSON.stringify(child) === JSON.stringify(condition),
                  ) ||
                  (bucket.value !== null &&
                    parameters.get(`filter[${field.name}]`) ===
                      String(bucket.value));
                return (
                  <button
                    key={index}
                    type="button"
                    className="facet-bucket"
                    aria-pressed={selected}
                    onClick={() => {
                      const remaining = children.filter(
                        (child) => !isFieldCondition(child),
                      );
                      if (!selected) remaining.push(condition);
                      update({
                        [`filter[${field.name}]`]: undefined,
                        where: remaining.length
                          ? JSON.stringify(
                              remaining.length === 1
                                ? remaining[0]
                                : { all: remaining },
                            )
                          : undefined,
                      });
                    }}
                  >
                    {bucket.value === null
                      ? t("No value")
                      : field.kind === "boolean"
                        ? bucket.value
                          ? t("Yes")
                          : t("No")
                        : String(bucket.value)}{" "}
                    ({formatNumber(bucket.count)})
                  </button>
                );
              })}
              {facet.other_count > 0
                ? t(" · Other ({0})", { 0: formatNumber(facet.other_count) })
                : ""}
            </div>
          )
        );
      })}
    </div>
  );
}

function facetExpression(raw: string | null): FilterExpression | undefined {
  try {
    return raw ? parseEditableFilter(raw) : undefined;
  } catch {
    return undefined;
  }
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
                aria-label={t(operator === "gte" ? "{0} from" : "{0} to", {
                  0: field.label,
                })}
                placeholder={operator === "gte" ? t("From") : t("To")}
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
          <option value="">{t("All")}</option>
          {values.map((value) => (
            <option key={value} value={value}>
              {field.kind === "boolean"
                ? value === "true"
                  ? t("Yes")
                  : t("No")
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
  // Hold the callback in a ref: callers pass an inline arrow, so depending on it
  // directly would restart the timer on every parent render and the filter value
  // would never be committed while the user keeps typing.
  const onValueChangeRef = useRef(onValueChange);
  useEffect(() => {
    onValueChangeRef.current = onValueChange;
  }, [onValueChange]);
  useEffect(() => {
    setDraft(value);
  }, [value]);
  useEffect(() => {
    if (draft === value) return;
    const timer = window.setTimeout(() => onValueChangeRef.current(draft), 300);
    return () => window.clearTimeout(timer);
  }, [draft, value]);
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
  const {
    options,
    pages,
    loadError,
    canLoad,
    isFetching,
    isPending,
    search,
    page,
    setSearch,
    setPage,
  } = useRelationOptions(target, value);
  return (
    <div className="filter-control">
      <span>{field.label}</span>
      <input
        value={search}
        placeholder={t("Search")}
        aria-label={t("Search {0}", { "0": field.label })}
        onChange={(event) => {
          setSearch(event.target.value);
          setPage(1);
        }}
      />
      <select
        value={value}
        aria-label={field.label}
        aria-busy={isFetching}
        aria-invalid={Boolean(loadError)}
        disabled={isPending && canLoad}
        onChange={(event) => onChange(event.target.value || undefined)}
      >
        <option value="">{t("All")}</option>
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
          onClick={() => setPage(page - 1)}
          aria-label={t("Previous options")}
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
          onClick={() => setPage(page + 1)}
          aria-label={t("Next options")}
        >
          <ChevronRight size={14} />
        </button>
      </span>
      <span className="sr-only" role="status" aria-live="polite">
        {isFetching
          ? t("Loading {0} options", { "0": field.label })
          : loadError ||
            t("{0} {1} options loaded", {
              0: formatNumber(options.length),
              1: field.label,
            })}
      </span>
    </div>
  );
}

function filterDisplayValue(
  value: string,
  kind: FieldDefinition["kind"],
): string {
  return String(
    toFormValue(value || undefined, {
      name: "filter",
      label: t("Filter"),
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
  const field = { name: "filter", label: t("Filter"), kind, required: false };
  return valueError(value, field)
    ? undefined
    : String(toApiValue(value, field));
}
