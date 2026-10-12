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
import type { FilterExpression, FilterOperator } from "../generated/client";
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
}

export function ResourceFilters({
  resource,
  fields,
  resources,
  searchParams,
  updateParam,
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
        />
      )}
    </>
  );
}

function FacetCounts({
  resource,
  fields,
  parameters,
}: {
  resource: ResourceDefinition;
  fields: FieldDefinition[];
  parameters: URLSearchParams;
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
      <p role="status">Filter counts are unavailable.</p>
    ) : null;
  return (
    <div className="facet-counts" aria-label="Counts for current filters">
      {facets.map((field) => {
        const facet = result.data.data[field.name];
        return (
          facet && (
            <span key={field.name}>
              <strong>{field.label}:</strong>{" "}
              {facet.buckets
                .map(
                  (bucket) =>
                    `${bucket.value === null ? "No value" : String(bucket.value)} (${bucket.count})`,
                )
                .join(" · ")}
              {facet.other_count > 0 ? ` · Other (${facet.other_count})` : ""}
            </span>
          )
        );
      })}
    </div>
  );
}

type FilterCondition = { field: string; op: FilterOperator; values?: string[] };

function AdvancedFilters({
  fields,
  raw,
  onApply,
}: {
  fields: FieldDefinition[];
  raw: string | null;
  onApply(value?: string): void;
}) {
  const [mode, setMode] = useState<"all" | "any">("all");
  const [rules, setRules] = useState<FilterCondition[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    setError("");
    if (!raw) {
      setRules([]);
      return;
    }
    try {
      const expression = JSON.parse(raw) as FilterExpression;
      const children =
        "all" in expression
          ? expression.all
          : "any" in expression
            ? expression.any
            : [expression];
      setMode("any" in expression ? "any" : "all");
      if (children.every((child): child is FilterCondition => "field" in child))
        setRules(children);
      else {
        setRules([]);
        setError(
          "This saved filter contains nested groups. Adding conditions will replace it.",
        );
      }
    } catch {
      setRules([]);
      setError("The saved filter is invalid.");
    }
  }, [raw]);
  if (!fields.length) return null;
  function change(index: number, patch: Partial<FilterCondition>) {
    setRules((current) =>
      current.map((rule, position) =>
        position === index ? { ...rule, ...patch } : rule,
      ),
    );
  }
  return (
    <details className="advanced-filters">
      <summary>Advanced filters{raw ? " · active" : ""}</summary>
      <label>
        Match{" "}
        <select
          aria-label="Filter match mode"
          value={mode}
          onChange={(event) => setMode(event.target.value as "all" | "any")}
        >
          <option value="all">All conditions</option>
          <option value="any">Any condition</option>
        </select>
      </label>
      {rules.map((rule, index) => {
        const field =
          fields.find((field) => field.name === rule.field) ?? fields[0];
        const choices = filterOperators(field);
        const noValue = rule.op === "is_null" || rule.op === "not_null";
        const list = rule.op === "in" || rule.op === "not_in";
        return (
          <div className="advanced-filter-row" key={index}>
            <select
              aria-label={`Condition ${index + 1} field`}
              value={rule.field}
              onChange={(event) =>
                change(index, {
                  field: event.target.value,
                  op: "eq",
                  values: [""],
                })
              }
            >
              {fields.map((field) => (
                <option key={field.name} value={field.name}>
                  {field.label}
                </option>
              ))}
            </select>
            <select
              aria-label={`Condition ${index + 1} operator`}
              value={rule.op}
              onChange={(event) => {
                const op = event.target.value as FilterOperator;
                change(index, {
                  op,
                  values: op === "is_null" || op === "not_null" ? [] : [""],
                });
              }}
            >
              {choices.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
            {!noValue && (field.kind === "enum" || field.kind === "boolean") ? (
              <select
                aria-label={`Condition ${index + 1} value`}
                multiple={list}
                value={list ? (rule.values ?? []) : (rule.values?.[0] ?? "")}
                onChange={(event) =>
                  change(index, {
                    values: list
                      ? Array.from(
                          event.target.selectedOptions,
                          (option) => option.value,
                        )
                      : [event.target.value],
                  })
                }
              >
                {!list && <option value="">Choose a value</option>}
                {(field.kind === "boolean"
                  ? ["true", "false"]
                  : (field.values ?? [])
                ).map((value) => (
                  <option key={value} value={value}>
                    {value}
                  </option>
                ))}
              </select>
            ) : (
              !noValue && (
                <input
                  aria-label={`Condition ${index + 1} value`}
                  value={
                    list
                      ? (rule.values ?? []).join(",")
                      : (rule.values?.[0] ?? "")
                  }
                  placeholder={
                    list
                      ? "Values separated by commas"
                      : field.kind === "json"
                        ? '{"key":"value"}'
                        : "Value"
                  }
                  onChange={(event) =>
                    change(index, {
                      values: list
                        ? event.target.value
                            .split(",")
                            .map((value) => value.trim())
                        : [event.target.value],
                    })
                  }
                />
              )
            )}
            <button
              type="button"
              className="button-secondary"
              aria-label={`Remove condition ${index + 1}`}
              onClick={() =>
                setRules((current) =>
                  current.filter((_, position) => position !== index),
                )
              }
            >
              Remove
            </button>
          </div>
        );
      })}
      <div className="advanced-filter-actions">
        <button
          type="button"
          className="button-secondary"
          disabled={rules.length >= 8}
          onClick={() =>
            setRules((current) => [
              ...current,
              { field: fields[0].name, op: "eq", values: [""] },
            ])
          }
        >
          Add condition
        </button>
        <button
          type="button"
          disabled={!rules.length}
          onClick={() => {
            if (
              rules.some(
                (rule) =>
                  !["is_null", "not_null"].includes(rule.op) &&
                  (!rule.values?.length || rule.values.some((value) => !value)),
              )
            ) {
              setError("Enter a value for each condition.");
              return;
            }
            setError("");
            onApply(JSON.stringify({ [mode]: rules }));
          }}
        >
          Apply filters
        </button>
        <button
          type="button"
          className="button-secondary"
          onClick={() => {
            setRules([]);
            setError("");
            onApply();
          }}
        >
          Clear advanced filters
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
    </details>
  );
}

export function filterOperators(
  field: FieldDefinition,
): [FilterOperator, string][] {
  const choices: [FilterOperator, string][] = [
    ["eq", "Equals"],
    ["ne", "Does not equal"],
    ["is_null", "Is empty"],
    ["not_null", "Is not empty"],
  ];
  if (field.kind !== "json")
    choices.push(["in", "Is one of"], ["not_in", "Is not one of"]);
  if (supportsRange(field))
    choices.push(
      ["gt", "Greater than"],
      ["gte", "At least"],
      ["lt", "Less than"],
      ["lte", "At most"],
    );
  if (["string", "text", "enum"].includes(field.kind))
    choices.push(
      ["contains", "Contains"],
      ["starts_with", "Starts with"],
      ["ends_with", "Ends with"],
    );
  if (field.kind === "json")
    choices.push(
      ["json_contains", "Contains JSON"],
      ["json_contained_by", "Is contained by JSON"],
      ["json_key", "Has key"],
    );
  return choices;
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
        aria-busy={isFetching}
        aria-invalid={Boolean(loadError)}
        disabled={isPending && canLoad}
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
          onClick={() => setPage(page - 1)}
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
          onClick={() => setPage(page + 1)}
          aria-label="Next options"
        >
          <ChevronRight size={14} />
        </button>
      </span>
      <span className="sr-only" role="status" aria-live="polite">
        {isFetching
          ? `Loading ${field.label} options`
          : loadError || `${options.length} ${field.label} options loaded`}
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
