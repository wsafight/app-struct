import { t } from "../../ui";
import type { FilterExpression, FilterOperator } from "../../generated/client";
import type { FieldDefinition } from "../../resource";
import { supportsRange } from "../../url-controller";
import { toApiValue } from "../../field-values";
import { csvValueError } from "./csv-import";

export type FilterCondition = Extract<FilterExpression, { field: string }>;
export type FilterGroup = Exclude<FilterExpression, FilterCondition>;
export const groupChildren = (group: FilterGroup) =>
  "all" in group ? group.all : group.any;
export const filterNodeCount = (node: FilterExpression): number =>
  "field" in node
    ? 1
    : 1 +
      groupChildren(node).reduce(
        (sum, child) => sum + filterNodeCount(child),
        0,
      );

export function parseEditableFilter(raw: string): FilterExpression {
  if (new TextEncoder().encode(raw).length > 16_384)
    throw new Error(t("Filter exceeds 16384 bytes."));
  const input: unknown = JSON.parse(raw);
  let nodes = 0;
  let values = 0;
  function parse(value: unknown, depth: number): FilterExpression {
    if (depth > 8 || ++nodes > 64)
      throw new Error(t("Filter exceeds 8 levels or 64 nodes."));
    if (!value || typeof value !== "object" || Array.isArray(value))
      throw new Error(t("Invalid filter condition."));
    const node = value as Record<string, unknown>;
    if ("all" in node || "any" in node) {
      const mode = "all" in node ? "all" : "any";
      if (
        Object.keys(node).length !== 1 ||
        !Array.isArray(node[mode]) ||
        !node[mode].length
      )
        throw new Error(t("Filter groups must contain conditions."));
      return {
        [mode]: node[mode].map((child) => parse(child, depth + 1)),
      } as FilterGroup;
    }
    if (
      typeof node.field !== "string" ||
      typeof node.op !== "string" ||
      !/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(node.field) ||
      Object.keys(node).some(
        (key) => !["field", "op", "values"].includes(key),
      ) ||
      (node.values !== undefined &&
        (!Array.isArray(node.values) ||
          node.values.some((value) => typeof value !== "string")))
    )
      throw new Error(t("Invalid filter condition."));
    const inputs = (node.values ?? []) as string[];
    values += inputs.length;
    if (values > 100) throw new Error(t("Filter exceeds 100 values."));
    return {
      field: node.field,
      op: node.op as FilterOperator,
      ...(inputs.length ? { values: inputs } : {}),
    };
  }
  return parse(input, 1);
}

export function filterConditionError(
  rule: FilterCondition,
  fields: FieldDefinition[],
): string | undefined {
  const field = fields.find((field) => field.name === rule.field);
  if (!field) return t("This field is unavailable or cannot be filtered.");
  if (!filterOperators(field).some(([op]) => op === rule.op))
    return t("This operator is not available for this field.");
  const noValue = ["is_null", "not_null"].includes(rule.op);
  const list = ["in", "not_in"].includes(rule.op);
  const values = rule.values ?? [];
  if (noValue)
    return values.length
      ? t("This operator does not accept a value.")
      : undefined;
  if ((!list && values.length !== 1) || !values.length)
    return t("Enter a value.");
  for (const value of values) {
    if (rule.op === "json_key") {
      if (!value) return t("Enter a key.");
      continue;
    }
    if (["contains", "starts_with", "ends_with"].includes(rule.op)) continue;
    if (field.kind === "datetime") {
      const error = csvValueError(value, { ...field, required: true });
      if (error) return error;
      continue;
    }
    try {
      if (field.kind === "boolean" && !["true", "false"].includes(value))
        return t("Choose Yes or No.");
      toApiValue(field.kind === "boolean" ? value === "true" : value, {
        ...field,
        required: !["string", "text"].includes(field.kind),
      });
    } catch (error) {
      return error instanceof Error ? error.message : t("Invalid value.");
    }
  }
  return undefined;
}

export function filterSummary(
  node: FilterExpression,
  fields: FieldDefinition[],
): string {
  if ("field" in node) {
    const field = fields.find((field) => field.name === node.field);
    const operator =
      field && filterOperators(field).find(([op]) => op === node.op)?.[1];
    return `${field?.label ?? node.field} ${operator ?? node.op}${node.values?.length ? ` ${node.values.join(", ")}` : ""}`;
  }
  return `(${groupChildren(node)
    .map((child) => filterSummary(child, fields))
    .join("all" in node ? " AND " : " OR ")})`;
}

export function filterOperators(
  field: FieldDefinition,
): [FilterOperator, string][] {
  const choices: [FilterOperator, string][] = [
    ["eq", t("Equals")],
    ["ne", t("Does not equal")],
    ["is_null", t("Is empty")],
    ["not_null", t("Is not empty")],
  ];
  if (field.kind !== "json")
    choices.push(["in", t("Is one of")], ["not_in", t("Is not one of")]);
  if (supportsRange(field))
    choices.push(
      ["gt", t("Greater than")],
      ["gte", t("At least")],
      ["lt", t("Less than")],
      ["lte", t("At most")],
    );
  if (["string", "text", "enum"].includes(field.kind))
    choices.push(
      ["contains", t("Contains")],
      ["starts_with", t("Starts with")],
      ["ends_with", t("Ends with")],
    );
  if (field.kind === "json")
    choices.push(
      ["json_contains", t("Contains JSON")],
      ["json_contained_by", t("Is contained by JSON")],
      ["json_key", t("Has key")],
    );
  return choices;
}
