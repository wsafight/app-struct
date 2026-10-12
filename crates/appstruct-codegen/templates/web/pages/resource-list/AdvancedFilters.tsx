import { useEffect, useState } from "react";
import { inputType, toFormValue, toApiValue } from "../../field-values";
import type { FilterExpression, FilterOperator } from "../../generated/client";
import type { FieldDefinition } from "../../resource";
import {
  filterConditionError,
  filterNodeCount,
  filterOperators,
  filterSummary,
  groupChildren,
  parseEditableFilter,
  type FilterCondition,
  type FilterGroup,
} from "./filter-expression";

export function AdvancedFilters({
  fields,
  raw,
  onApply,
}: {
  fields: FieldDefinition[];
  raw: string | null;
  onApply(value?: string): void;
}) {
  const [tree, setTree] = useState<FilterGroup>({ all: [] });
  const [error, setError] = useState("");
  const [invalidSaved, setInvalidSaved] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  useEffect(() => {
    setError("");
    setSubmitted(false);
    setInvalidSaved(false);
    try {
      const parsed = raw ? parseEditableFilter(raw) : { all: [] };
      setTree("field" in parsed ? { all: [parsed] } : parsed);
    } catch (error) {
      setInvalidSaved(true);
      setError(
        error instanceof Error ? error.message : "The saved filter is invalid.",
      );
    }
  }, [raw]);
  if (!fields.length) return null;
  const count = filterNodeCount(tree);
  function validate(node: FilterExpression): boolean {
    return "field" in node
      ? !filterConditionError(node, fields)
      : groupChildren(node).length > 0 && groupChildren(node).every(validate);
  }
  function apply() {
    setSubmitted(true);
    if (!validate(tree)) {
      setError(
        "Correct the highlighted conditions; each group needs at least one condition.",
      );
      return;
    }
    try {
      const serialized = JSON.stringify(tree);
      parseEditableFilter(serialized);
      setError("");
      onApply(serialized);
    } catch (error) {
      setError(error instanceof Error ? error.message : "Invalid filters.");
    }
  }
  return (
    <details className="advanced-filters">
      <summary>Advanced filters{raw ? " · active" : ""}</summary>
      {raw && !invalidSaved && (
        <p className="filter-summary">{filterSummary(tree, fields)}</p>
      )}
      {!invalidSaved && (
        <GroupEditor
          group={tree}
          fields={fields}
          path=""
          depth={1}
          nodeCount={count}
          submitted={submitted}
          onChange={setTree}
        />
      )}
      <div className="advanced-filter-actions">
        <button
          type="button"
          className="primary-button"
          disabled={invalidSaved || !groupChildren(tree).length}
          onClick={apply}
        >
          Apply filters
        </button>
        <button
          type="button"
          className="secondary-button"
          onClick={() => {
            setTree({ all: [] });
            setError("");
            setInvalidSaved(false);
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

function GroupEditor({
  group,
  fields,
  path,
  depth,
  nodeCount,
  submitted,
  onChange,
}: {
  group: FilterGroup;
  fields: FieldDefinition[];
  path: string;
  depth: number;
  nodeCount: number;
  submitted: boolean;
  onChange(group: FilterGroup): void;
}) {
  const mode = "all" in group ? "all" : "any";
  const children = groupChildren(group);
  const label = path ? `Group ${path}` : "Filter";
  function replace(next: FilterExpression[], nextMode = mode) {
    onChange({ [nextMode]: next } as FilterGroup);
  }
  function change(index: number, child: FilterExpression) {
    replace(
      children.map((current, position) =>
        position === index ? child : current,
      ),
    );
  }
  return (
    <fieldset className="filter-group">
      <legend>{path ? label : "Conditions"}</legend>
      <label>
        Match{" "}
        <select
          aria-label={path ? `${label} match mode` : "Filter match mode"}
          value={mode}
          onChange={(event) =>
            replace(children, event.target.value as "all" | "any")
          }
        >
          <option value="all">All conditions</option>
          <option value="any">Any condition</option>
        </select>
      </label>
      {children.map((child, index) => {
        const childPath = path ? `${path}.${index + 1}` : String(index + 1);
        return (
          <div key={index} className="filter-node">
            {"field" in child ? (
              <ConditionEditor
                rule={child}
                fields={fields}
                path={childPath}
                submitted={submitted}
                onChange={(rule) => change(index, rule)}
              />
            ) : (
              <GroupEditor
                group={child}
                fields={fields}
                path={childPath}
                depth={depth + 1}
                nodeCount={nodeCount}
                submitted={submitted}
                onChange={(next) => change(index, next)}
              />
            )}
            <button
              type="button"
              className="secondary-button"
              aria-label={`Remove ${"field" in child ? "condition" : "group"} ${childPath}`}
              onClick={() =>
                replace(children.filter((_, position) => position !== index))
              }
            >
              Remove
            </button>
          </div>
        );
      })}
      {submitted && !children.length && (
        <p role="alert">Add a condition to {label.toLowerCase()}.</p>
      )}
      <div className="advanced-filter-actions">
        <button
          type="button"
          className="secondary-button"
          aria-label={path ? `Add condition to group ${path}` : "Add condition"}
          disabled={nodeCount >= 64 || depth >= 8}
          onClick={() =>
            replace([
              ...children,
              { field: fields[0].name, op: "eq", values: [""] },
            ])
          }
        >
          Add condition
        </button>
        <button
          type="button"
          className="secondary-button"
          aria-label={path ? `Add group to group ${path}` : "Add group"}
          disabled={nodeCount >= 63 || depth >= 7}
          onClick={() =>
            replace([
              ...children,
              { all: [{ field: fields[0].name, op: "eq", values: [""] }] },
            ])
          }
        >
          Add group
        </button>
      </div>
    </fieldset>
  );
}

function ConditionEditor({
  rule,
  fields,
  path,
  submitted,
  onChange,
}: {
  rule: FilterCondition;
  fields: FieldDefinition[];
  path: string;
  submitted: boolean;
  onChange(rule: FilterCondition): void;
}) {
  const field = fields.find((field) => field.name === rule.field);
  const operators = field ? filterOperators(field) : [];
  const list = ["in", "not_in"].includes(rule.op);
  const noValue = ["is_null", "not_null"].includes(rule.op);
  const error = submitted ? filterConditionError(rule, fields) : undefined;
  const label = `Condition ${path}`;
  return (
    <div className="advanced-filter-row">
      <select
        aria-label={`${label} field`}
        value={rule.field}
        onChange={(event) =>
          onChange({ field: event.target.value, op: "eq", values: [""] })
        }
      >
        {!field && (
          <option value={rule.field}>{rule.field} (unavailable)</option>
        )}
        {fields.map((field) => (
          <option key={field.name} value={field.name}>
            {field.label}
          </option>
        ))}
      </select>
      <select
        aria-label={`${label} operator`}
        value={rule.op}
        onChange={(event) => {
          const op = event.target.value as FilterOperator;
          onChange({
            ...rule,
            op,
            values: ["is_null", "not_null"].includes(op) ? [] : [""],
          });
        }}
      >
        {!operators.some(([op]) => op === rule.op) && (
          <option value={rule.op}>{rule.op} (unavailable)</option>
        )}
        {operators.map(([op, label]) => (
          <option key={op} value={op}>
            {label}
          </option>
        ))}
      </select>
      {!noValue && (field?.kind === "enum" || field?.kind === "boolean") ? (
        <select
          aria-label={`${label} value`}
          aria-invalid={Boolean(error)}
          multiple={list}
          value={list ? (rule.values ?? []) : (rule.values?.[0] ?? "")}
          onChange={(event) =>
            onChange({
              ...rule,
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
              {field.kind === "boolean"
                ? value === "true"
                  ? "Yes"
                  : "No"
                : value}
            </option>
          ))}
        </select>
      ) : (
        !noValue && (
          <input
            aria-label={`${label} value`}
            aria-invalid={Boolean(error)}
            type={!list && field ? inputType(field.kind) : "text"}
            step="any"
            value={
              list
                ? (rule.values ?? []).join(",")
                : field?.kind === "datetime"
                  ? String(toFormValue(rule.values?.[0], field))
                  : (rule.values?.[0] ?? "")
            }
            placeholder={
              list
                ? "Values separated by commas"
                : field?.kind === "json"
                  ? '{"key":"value"}'
                  : "Value"
            }
            onChange={(event) => {
              let value = event.target.value;
              if (!list && field?.kind === "datetime" && value) {
                try {
                  value = String(
                    toApiValue(value, { ...field, required: true }),
                  );
                } catch {
                  /* Show incomplete input until Apply. */
                }
              }
              onChange({
                ...rule,
                values: list
                  ? value.split(",").map((value) => value.trim())
                  : [value],
              });
            }}
          />
        )
      )}
      {error && (
        <small className="field-error" role="alert">
          {label}: {error}
        </small>
      )}
    </div>
  );
}
