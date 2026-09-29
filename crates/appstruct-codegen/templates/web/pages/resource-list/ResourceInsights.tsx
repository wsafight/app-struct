import { useQuery } from "@tanstack/react-query";
import { BarChart3, ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import {
  Bar,
  BarChart,
  Cell,
  CartesianGrid,
  Legend,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { AggregateQuery } from "../../generated/client";
import { resourceQueryKeys } from "../../query";
import type {
  AccessActor,
  ChartType,
  FieldDefinition,
  ResourceDefinition,
} from "../../resource";
import { canAccessRule, errorMessage, useResourceActor } from "../../resource";

interface MetricOption {
  value: string;
  label: string;
}

interface GroupOption {
  value: string;
  label: string;
}

interface ChartRow {
  dimension: string;
  value: number;
}

const chartColors = [
  "#167565",
  "#286690",
  "#a45b2a",
  "#7a4d8b",
  "#477238",
  "#a43d4f",
];

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

export function aggregateGroupOptions(
  fields: FieldDefinition[],
  resources: ResourceDefinition[],
  actor: AccessActor | null,
): GroupOption[] {
  const options: GroupOption[] = [];
  for (const field of fields) {
    if (field.kind !== "json" && field.kind !== "relation") {
      options.push({ value: field.name, label: field.label });
      continue;
    }
    if (field.kind !== "relation" || !field.relation) continue;
    const target = resources.find((resource) => resource.id === field.relation);
    if (!target || !canAccessRule(target.access.list, actor)) continue;
    for (const targetField of target.fields) {
      if (
        !targetField.filterable ||
        targetField.kind === "json" ||
        targetField.kind === "relation" ||
        !canAccessRule(targetField.readAccess ?? { mode: "public" }, actor)
      )
        continue;
      options.push({
        value: `${field.apiName}.${targetField.name}`,
        label: `${field.label} / ${targetField.label}`,
      });
    }
  }
  return options;
}

export function aggregateGroupKey(groupBy: string): string {
  return `group_${groupBy.replaceAll(".", "_")}`;
}

export function aggregateChartRows(
  rows: Record<string, unknown>[],
  groupBy: string,
  metricKey: string,
): ChartRow[] | null {
  const groupKey = aggregateGroupKey(groupBy);
  const chartRows: ChartRow[] = [];
  for (const row of rows) {
    const raw = row[metricKey];
    const value = typeof raw === "number" ? raw : Number(raw);
    if (!Number.isFinite(value)) return null;
    chartRows.push({
      dimension: formatAggregateValue(row[groupKey]),
      value,
    });
  }
  return chartRows;
}

export function ResourceInsights({
  resource,
  resources,
  fields,
  query,
}: {
  resource: ResourceDefinition;
  resources: ResourceDefinition[];
  fields: FieldDefinition[];
  query: Pick<AggregateQuery, "q" | "filters" | "range_filters">;
}) {
  const actor = useResourceActor();
  const metrics = aggregateMetricOptions(fields);
  const groups = aggregateGroupOptions(fields, resources, actor);
  const charts = (resource.charts ?? []).filter(
    (chart) =>
      metrics.some((option) => option.value === chart.measure) &&
      (!chart.dimension ||
        groups.some((option) => option.value === chart.dimension)),
  );
  const initialChart = charts[0];
  const [open, setOpen] = useState(false);
  const [selectedChart, setSelectedChart] = useState(initialChart?.name ?? "");
  const [metric, setMetric] = useState(initialChart?.measure ?? "count");
  const [groupBy, setGroupBy] = useState(initialChart?.dimension ?? "");
  const [chartType, setChartType] = useState<ChartType>(
    initialChart?.type ?? "horizontal_bar",
  );
  const [limit, setLimit] = useState(initialChart?.limit ?? 20);
  const aggregateQuery: AggregateQuery = {
    ...query,
    metrics: [metric],
    group_by: groupBy ? [groupBy] : undefined,
    limit,
    order: "desc",
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
    groups.find((option) => option.value === groupBy)?.label ?? groupBy;
  const metricKey = metric === "count" ? "count" : metric.replace(":", "_");
  const aggregateRows = aggregate.data?.data ?? [];
  const chartRows = groupBy
    ? aggregateChartRows(aggregateRows, groupBy, metricKey)
    : null;

  function chooseChart(name: string) {
    setSelectedChart(name);
    const chart = charts.find((candidate) => candidate.name === name);
    if (!chart) return;
    setMetric(chart.measure);
    setGroupBy(chart.dimension ?? "");
    setChartType(chart.type);
    setLimit(chart.limit);
  }

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
            {charts.length > 0 && (
              <label>
                Chart
                <select
                  value={selectedChart}
                  onChange={(event) => chooseChart(event.target.value)}
                >
                  <option value="">Ad hoc</option>
                  {charts.map((chart) => (
                    <option key={chart.name} value={chart.name}>
                      {chart.label}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <label>
              Metric
              <select
                value={metric}
                onChange={(event) => {
                  setSelectedChart("");
                  setMetric(event.target.value);
                }}
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
                onChange={(event) => {
                  setSelectedChart("");
                  setGroupBy(event.target.value);
                  setChartType("horizontal_bar");
                  setLimit(20);
                }}
              >
                <option value="">No grouping</option>
                {groups.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
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
          {!aggregate.isPending &&
            !aggregate.error &&
            groupBy &&
            chartRows &&
            chartRows.length > 0 && (
              <AggregateChart
                type={chartType}
                rows={chartRows}
                metricLabel={metricLabel}
                groupLabel={groupLabel}
              />
            )}
          {!aggregate.isPending &&
            !aggregate.error &&
            groupBy &&
            chartRows &&
            chartRows.length === 0 && (
              <div className="empty">No summary data</div>
            )}
          {!aggregate.isPending &&
            !aggregate.error &&
            groupBy &&
            !chartRows && (
              <div className="table-frame insights-table">
                <table>
                  <thead>
                    <tr>
                      <th>{groupLabel}</th>
                      <th>{metricLabel}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {aggregateRows.map((row, index) => (
                      <tr
                        key={`${String(row[aggregateGroupKey(groupBy)])}-${index}`}
                      >
                        <td>
                          {formatAggregateValue(
                            row[aggregateGroupKey(groupBy)],
                          )}
                        </td>
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

function AggregateChart({
  type,
  rows,
  metricLabel,
  groupLabel,
}: {
  type: ChartType;
  rows: ChartRow[];
  metricLabel: string;
  groupLabel: string;
}) {
  const height =
    type === "horizontal_bar"
      ? Math.min(420, Math.max(220, rows.length * 38))
      : 300;
  const label = `${metricLabel} by ${groupLabel}`;
  if (type === "donut") {
    return (
      <div
        className="insights-chart insights-chart-donut"
        style={{ height }}
        role="img"
        aria-label={label}
      >
        <ResponsiveContainer width="100%" height="100%">
          <PieChart accessibilityLayer>
            <Pie
              data={rows}
              dataKey="value"
              nameKey="dimension"
              innerRadius="46%"
              outerRadius="76%"
              paddingAngle={1}
              isAnimationActive={false}
            >
              {rows.map((row, index) => (
                <Cell
                  key={`${row.dimension}-${index}`}
                  fill={chartColors[index % chartColors.length]}
                />
              ))}
            </Pie>
            <Tooltip />
            <Legend />
          </PieChart>
        </ResponsiveContainer>
      </div>
    );
  }
  const horizontal = type === "horizontal_bar";
  return (
    <div
      className="insights-chart"
      style={{ height }}
      role="img"
      aria-label={label}
    >
      <ResponsiveContainer width="100%" height="100%">
        <BarChart
          data={rows}
          layout={horizontal ? "vertical" : "horizontal"}
          margin={{ top: 4, right: 20, bottom: 4, left: 12 }}
          accessibilityLayer
        >
          <CartesianGrid
            strokeDasharray="3 3"
            horizontal={!horizontal}
            vertical={horizontal}
          />
          {horizontal ? (
            <>
              <XAxis type="number" />
              <YAxis
                dataKey="dimension"
                type="category"
                width={140}
                tick={{ fontSize: 12 }}
              />
            </>
          ) : (
            <>
              <XAxis
                dataKey="dimension"
                type="category"
                tick={{ fontSize: 12 }}
              />
              <YAxis type="number" />
            </>
          )}
          <Tooltip />
          <Bar
            dataKey="value"
            name={metricLabel}
            fill="var(--color-accent)"
            radius={horizontal ? [0, 3, 3, 0] : [3, 3, 0, 0]}
            isAnimationActive={false}
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

function formatAggregateValue(value: unknown): string {
  if (value === null || value === undefined) return "-";
  return typeof value === "number"
    ? value.toLocaleString(undefined, { maximumFractionDigits: 4 })
    : String(value);
}
