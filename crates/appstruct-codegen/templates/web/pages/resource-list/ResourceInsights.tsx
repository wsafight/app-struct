import { formatNumber } from "../../ui";
import { t } from "../../ui";
import { useQuery } from "@tanstack/react-query";
import { BarChart3, ChevronDown, ChevronRight } from "lucide-react";
import { lazy, Suspense } from "react";
import Decimal from "decimal.js";
import { Link, useNavigate } from "../../navigation";
import { reportDrilldown } from "./report-query";
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

// `recharts` is the largest dependency in the generated Web bundle and is only needed
// once a summary panel is expanded, so the chart module is loaded on demand.
const AggregateChart = lazy(() =>
  import("./AggregateChart").then((module) => ({
    default: module.AggregateChart,
  })),
);

export function aggregateMetricOptions(
  fields: FieldDefinition[],
): MetricOption[] {
  const options: MetricOption[] = [{ value: "count", label: t("Count") }];
  for (const field of fields) {
    if (["integer", "bigint", "decimal"].includes(field.kind)) {
      options.push(
        {
          value: `sum:${field.name}`,
          label: t("Sum {0}", { "0": field.label }),
        },
        {
          value: `avg:${field.name}`,
          label: t("Average {0}", { "0": field.label }),
        },
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
        {
          value: `min:${field.name}`,
          label: t("Minimum {0}", { "0": field.label }),
        },
        {
          value: `max:${field.name}`,
          label: t("Maximum {0}", { "0": field.label }),
        },
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
    if (raw === null || raw === undefined || !Number.isFinite(value))
      return null;
    if (typeof raw === "string") {
      try {
        if (!new Decimal(raw).eq(String(value))) return null;
      } catch {
        return null;
      }
    }
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
  parameters,
  updateParams,
}: {
  resource: ResourceDefinition;
  resources: ResourceDefinition[];
  fields: FieldDefinition[];
  query: Pick<AggregateQuery, "q" | "filters" | "range_filters" | "where">;
  parameters: URLSearchParams;
  updateParams(values: Record<string, string | undefined>): void;
}) {
  const actor = useResourceActor();
  const navigate = useNavigate();
  const metrics = aggregateMetricOptions(fields);
  const groups = aggregateGroupOptions(fields, resources, actor);
  const charts = (resource.charts ?? []).filter(
    (chart) =>
      metrics.some((option) => option.value === chart.measure) &&
      (!chart.dimension ||
        groups.some((option) => option.value === chart.dimension)),
  );
  const selectedChart = charts.find(
    (chart) => chart.name === parameters.get("report_chart"),
  );
  const initial = selectedChart ?? charts[0];
  const open = parameters.get("summary") === "1";
  const requestedMetric =
    parameters.get("report_metric") ?? initial?.measure ?? "count";
  const metric = metrics.some((option) => option.value === requestedMetric)
    ? requestedMetric
    : "count";
  const requestedGroup =
    parameters.get("report_group") ?? initial?.dimension ?? "";
  const groupBy = groups.some((option) => option.value === requestedGroup)
    ? requestedGroup
    : "";
  const groupField = fields.find((field) => field.name === groupBy);
  const requestedBucket = parameters.get("report_bucket");
  const bucket =
    groupField &&
    ["date", "datetime"].includes(groupField.kind) &&
    ["day", "week", "month"].includes(requestedBucket ?? "")
      ? (requestedBucket as AggregateQuery["time_bucket"])
      : undefined;
  const requestedType =
    parameters.get("report_type") ?? initial?.type ?? "horizontal_bar";
  const chartType: ChartType = [
    "bar",
    "horizontal_bar",
    "donut",
    "kpi",
  ].includes(requestedType)
    ? (requestedType as ChartType)
    : "horizontal_bar";
  const requestedLimit = Number(
    parameters.get("report_limit") ?? initial?.limit ?? 20,
  );
  const limit =
    Number.isInteger(requestedLimit) &&
    requestedLimit >= 1 &&
    requestedLimit <= 500
      ? requestedLimit
      : 20;
  const aggregateQuery: AggregateQuery = {
    ...query,
    metrics: [metric],
    group_by: groupBy ? [groupBy] : undefined,
    limit,
    time_bucket: bucket,
    order_by: bucket ? "group" : "metric",
    order: bucket ? "asc" : "desc",
  };
  const aggregate = useQuery({
    queryKey: resourceQueryKeys.aggregate(
      resource.id,
      JSON.stringify(aggregateQuery),
    ),
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
  const drilldowns = aggregateRows.map((row) =>
    reportDrilldown(
      parameters,
      groupBy,
      row[aggregateGroupKey(groupBy)],
      bucket,
      groupField?.kind,
    ),
  );
  const change = (values: Record<string, string | undefined>) =>
    updateParams({ report_chart: undefined, ...values });

  return (
    <section className="resource-insights" aria-label={t("Resource summary")}>
      <button
        type="button"
        className="insights-toggle"
        aria-expanded={open}
        onClick={() => updateParams({ summary: open ? undefined : "1" })}
      >
        {open ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
        <BarChart3 size={16} /> {t("Summary")}
      </button>
      {open && (
        <div className="insights-body">
          <div className="insights-controls">
            {charts.length > 0 && (
              <label>
                {t("Chart")}{" "}
                <select
                  aria-label={t("Report chart")}
                  value={selectedChart?.name ?? ""}
                  onChange={(event) => {
                    const chart = charts.find(
                      (chart) => chart.name === event.target.value,
                    );
                    if (chart)
                      updateParams({
                        report_chart: chart.name,
                        report_metric: chart.measure,
                        report_group: chart.dimension ?? "none",
                        report_type: chart.type,
                        report_limit: String(chart.limit),
                        report_bucket: undefined,
                      });
                    else updateParams({ report_chart: undefined });
                  }}
                >
                  <option value="">{t("Ad hoc")}</option>
                  {charts.map((chart) => (
                    <option key={chart.name} value={chart.name}>
                      {chart.label}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <label>
              {t("Metric")}
              <select
                aria-label={t("Report metric")}
                value={metric}
                onChange={(event) =>
                  change({ report_metric: event.target.value })
                }
              >
                {metrics.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {" "}
              {t("Group by")}
              <select
                aria-label={t("Report group")}
                value={groupBy}
                onChange={(event) =>
                  change({
                    report_group: event.target.value || "none",
                    report_bucket: undefined,
                    report_type: "horizontal_bar",
                  })
                }
              >
                <option value=""> {t("No grouping")}</option>
                {groups.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            {groupField && ["date", "datetime"].includes(groupField.kind) && (
              <label>
                {" "}
                {t("Period")}
                <select
                  aria-label={t("Report period")}
                  value={bucket ?? ""}
                  onChange={(event) =>
                    change({
                      report_bucket: event.target.value || undefined,
                      report_type: "bar",
                    })
                  }
                >
                  <option value=""> {t("Exact value")}</option>
                  <option value="day"> {t("Day")} </option>
                  <option value="week">{t("Week")}</option>
                  <option value="month"> {t("Month")} </option>
                </select>
              </label>
            )}
            {groupBy && (
              <label>
                {t("Display")}
                <select
                  aria-label={t("Report display")}
                  value={chartType === "kpi" ? "horizontal_bar" : chartType}
                  onChange={(event) =>
                    change({ report_type: event.target.value })
                  }
                >
                  <option value="bar">{t("Bars")}</option>
                  <option value="horizontal_bar">{t("Horizontal bars")}</option>
                  <option value="donut">{t("Donut")}</option>
                </select>
              </label>
            )}
            <label>
              {t("Groups")}
              <select
                aria-label={t("Report group limit")}
                value={limit}
                onChange={(event) =>
                  change({ report_limit: event.target.value })
                }
              >
                {Array.from(new Set([20, 50, 100, 500, limit]))
                  .sort((a, b) => a - b)
                  .map((count) => (
                    <option key={count} value={count}>
                      {count}
                    </option>
                  ))}
              </select>
            </label>
          </div>
          <p className="report-help">
            {t("Save this report with Saved views.")}{" "}
            {bucket ? t("Periods use UTC; weeks start on Monday. ") : ""}
            {groupBy
              ? t(
                  "Showing up to {0} nonempty groups; choose a date range to focus a trend.",
                  { "0": limit },
                )
              : ""}
          </p>
          {aggregate.isPending && (
            <div className="empty"> {t("Loading summary...")}</div>
          )}
          {aggregate.error && (
            <div className="alert" role="alert">
              {errorMessage(aggregate.error)}
            </div>
          )}
          {!aggregate.isPending && !aggregate.error && groupBy && (
            <>
              {chartRows && chartRows.length > 0 && (
                <Suspense
                  fallback={
                    <div className="empty">{t("Loading chart...")} </div>
                  }
                >
                  <AggregateChart
                    type={chartType}
                    rows={chartRows}
                    metricLabel={metricLabel}
                    groupLabel={groupLabel}
                    onSelect={(index) => {
                      const next = drilldowns[index];
                      if (next !== undefined)
                        void navigate(`/${resource.slug}?${next}`);
                    }}
                  />
                </Suspense>
              )}
              {!aggregateRows.length ? (
                <div className="empty">{t("No summary data")}</div>
              ) : (
                <div className="table-frame insights-table">
                  <table>
                    <thead>
                      <tr>
                        <th>{groupLabel}</th>
                        <th>{metricLabel}</th>
                        <th>{t("Records")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {aggregateRows.map((row, index) => (
                        <tr key={index}>
                          <td>
                            {formatAggregateValue(
                              row[aggregateGroupKey(groupBy)],
                            )}
                          </td>
                          <td>{formatAggregateValue(row[metricKey])}</td>
                          <td>
                            {drilldowns[index] !== undefined && (
                              <Link
                                to={`/${resource.slug}?${drilldowns[index]}`}
                              >
                                {t("View matching records")}
                              </Link>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </>
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
    ? formatNumber(value, { maximumFractionDigits: 4 })
    : String(value);
}
