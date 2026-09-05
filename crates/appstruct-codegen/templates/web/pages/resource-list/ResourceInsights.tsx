import { useQuery } from "@tanstack/react-query";
import { BarChart3, ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { AggregateQuery } from "../../generated/client";
import { resourceQueryKeys } from "../../query";
import type { FieldDefinition, ResourceDefinition } from "../../resource";
import { errorMessage } from "../../resource";

interface MetricOption {
  value: string;
  label: string;
}

export function aggregateMetricOptions(
  fields: FieldDefinition[],
): MetricOption[] {
  const options: MetricOption[] = [{ value: "count", label: "Count" }];
  for (const field of fields) {
    if (["integer", "bigint", "decimal"].includes(field.kind)) {
      options.push(
        { value: `sum:${field.name}`, label: `Sum ${field.label}` },
        { value: `avg:${field.name}`, label: `Average ${field.label}` },
      );
    }
    if (
      [
        "integer",
        "bigint",
        "decimal",
        "string",
        "enum",
        "date",
        "datetime",
      ].includes(field.kind)
    ) {
      options.push(
        { value: `min:${field.name}`, label: `Minimum ${field.label}` },
        { value: `max:${field.name}`, label: `Maximum ${field.label}` },
      );
    }
  }
  return options;
}

export function ResourceInsights({
  resource,
  fields,
  query,
}: {
  resource: ResourceDefinition;
  fields: FieldDefinition[];
  query: Pick<AggregateQuery, "q" | "filters" | "range_filters">;
}) {
  const [open, setOpen] = useState(false);
  const [metric, setMetric] = useState("count");
  const [groupBy, setGroupBy] = useState("");
  const metrics = aggregateMetricOptions(fields);
  const groups = fields.filter(
    (field) => field.kind !== "json" && field.kind !== "relation",
  );
  const aggregateQuery: AggregateQuery = {
    ...query,
    metrics: [metric],
    group_by: groupBy ? [groupBy] : undefined,
    limit: 100,
  };
  const cacheKey = JSON.stringify(aggregateQuery);
  const aggregate = useQuery({
    queryKey: resourceQueryKeys.aggregate(resource.id, cacheKey),
    queryFn: ({ signal }) => resource.api.aggregate(aggregateQuery, { signal }),
    enabled: open,
  });
  const metricLabel =
    metrics.find((option) => option.value === metric)?.label ?? metric;
  const groupLabel =
    fields.find((field) => field.name === groupBy)?.label ?? groupBy;
  const metricKey = metric === "count" ? "count" : metric.replace(":", "_");

  return (
    <section className="resource-insights" aria-label="Resource summary">
      <button
        type="button"
        className="insights-toggle"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {open ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
        <BarChart3 size={16} /> Summary
      </button>
      {open && (
        <div className="insights-body">
          <div className="insights-controls">
            <label>
              Metric
              <select
                value={metric}
                onChange={(event) => setMetric(event.target.value)}
              >
                {metrics.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Group by
              <select
                value={groupBy}
                onChange={(event) => setGroupBy(event.target.value)}
              >
                <option value="">No grouping</option>
                {groups.map((field) => (
                  <option key={field.name} value={field.name}>
                    {field.label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {aggregate.isPending && (
            <div className="empty">Loading summary...</div>
          )}
          {aggregate.error && (
            <div className="alert" role="alert">
              {errorMessage(aggregate.error)}
            </div>
          )}
          {!aggregate.isPending && !aggregate.error && groupBy && (
            <div className="table-frame insights-table">
              <table>
                <thead>
                  <tr>
                    <th>{groupLabel}</th>
                    <th>{metricLabel}</th>
                  </tr>
                </thead>
                <tbody>
                  {(aggregate.data?.data ?? []).map((row, index) => (
                    <tr key={`${String(row[`group_${groupBy}`])}-${index}`}>
                      <td>{formatAggregateValue(row[`group_${groupBy}`])}</td>
                      <td>{formatAggregateValue(row[metricKey])}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {!aggregate.isPending && !aggregate.error && !groupBy && (
            <div className="insight-value">
              <span>{metricLabel}</span>
              <strong>
                {formatAggregateValue(aggregate.data?.data[0]?.[metricKey])}
              </strong>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

function formatAggregateValue(value: unknown): string {
  if (value === null || value === undefined) return "-";
  return typeof value === "number"
    ? value.toLocaleString(undefined, { maximumFractionDigits: 4 })
    : String(value);
}
