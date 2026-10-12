import type { AggregateQuery, FilterExpression } from "../../generated/client";
import { parseEditableFilter } from "./filter-expression";

export function reportDrilldown(
  parameters: URLSearchParams,
  group: string,
  value: unknown,
  bucket?: AggregateQuery["time_bucket"],
  kind?: string,
): string | undefined {
  if (!group || (group.includes(".") && value == null)) return undefined;
  const next = new URLSearchParams(parameters);
  next.delete("page");
  next.delete("cursor");
  next.delete("direction");
  next.delete("summary");
  if (group.includes(".")) {
    next.set(`filter[${group}]`, String(value));
  } else {
    let condition: FilterExpression;
    if (value == null) condition = { field: group, op: "is_null" };
    else if (bucket) {
      const start = new Date(`${value}T00:00:00Z`);
      if (Number.isNaN(start.getTime())) return undefined;
      const end = new Date(start);
      if (bucket === "month") end.setUTCMonth(end.getUTCMonth() + 1);
      else end.setUTCDate(end.getUTCDate() + (bucket === "week" ? 7 : 1));
      const format = (date: Date) =>
        kind === "date" ? date.toISOString().slice(0, 10) : date.toISOString();
      condition = {
        all: [
          { field: group, op: "gte", values: [format(start)] },
          { field: group, op: "lt", values: [format(end)] },
        ],
      };
    } else condition = { field: group, op: "eq", values: [String(value)] };
    try {
      const raw = parameters.get("where");
      const existing = raw ? parseEditableFilter(raw) : undefined;
      const expression: FilterExpression = existing
        ? {
            all: [
              ...("all" in existing ? existing.all : [existing]),
              condition,
            ],
          }
        : condition;
      const serialized = JSON.stringify(expression);
      parseEditableFilter(serialized);
      next.set("where", serialized);
    } catch {
      return undefined;
    }
  }
  return next.toString();
}
