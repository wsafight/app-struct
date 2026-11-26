import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense } from "react";
import type { AggregateQuery } from "../generated/client";
import { resources } from "../generated/resources";
import { Link, useNavigate } from "../navigation";
import {
  aggregateChartRows,
  aggregateGroupOptions,
  aggregateMetricOptions,
} from "../pages/resource-list/ResourceInsights";
import { resourceQueryKeys } from "../query";
import {
  canAccessResource,
  errorMessage,
  type ChartDefinition,
  type ResourceDefinition,
  useResourceActor,
} from "../resource";
import { formatNumber, t } from "../ui";

const AggregateChart = lazy(() =>
  import("../pages/resource-list/AggregateChart").then((module) => ({
    default: module.AggregateChart,
  })),
);

interface DashboardWidgetConfig {
  id: string;
  title?: string | null;
  entityId: string;
  chart: string;
  span: number;
}

interface DashboardConfig {
  title: string;
  widgets: DashboardWidgetConfig[];
}

const dashboardConfig: DashboardConfig = __DASHBOARD_CONFIG__;

export function DashboardPage() {
  const actor = useResourceActor();
  const visibleWidgets = dashboardConfig.widgets
    .map((widget) => {
      const resource = resources.find((item) => item.id === widget.entityId);
      const chart = resource?.charts?.find((item) => item.name === widget.chart);
      if (!resource || !chart || !canAccessResource(resource, "list", actor))
        return null;
      return { widget, resource, chart };
    })
    .filter(
      (
        item,
      ): item is {
        widget: DashboardWidgetConfig;
        resource: ResourceDefinition;
        chart: ChartDefinition;
      } => item !== null,
    );

  return (
    <main className="page">
      <header className="page-header">
        <div>
          <h1>{t(dashboardConfig.title)}</h1>
          <p className="muted">
            {t("Live metrics across your authorized resources.")}
          </p>
        </div>
      </header>
      {visibleWidgets.length === 0 ? (
        <div className="empty">{t("No dashboard widgets are available.")}</div>
      ) : (
        <section className="dashboard-grid" aria-label={t(dashboardConfig.title)}>
          {visibleWidgets.map(({ widget, resource, chart }) => (
            <DashboardWidgetCard
              key={widget.id}
              widget={widget}
              resource={resource}
              chart={chart}
            />
          ))}
        </section>
      )}
    </main>
  );
}

function DashboardWidgetCard({
  widget,
  resource,
  chart,
}: {
  widget: DashboardWidgetConfig;
  resource: ResourceDefinition;
  chart: ChartDefinition;
}) {
  const actor = useResourceActor();
  const navigate = useNavigate();
  const metrics = aggregateMetricOptions(resource.fields);
  const groups = aggregateGroupOptions(resource.fields, resources, actor);
  const groupBy = chart.dimension ?? "";
  const metric = chart.measure;
  const aggregateQuery: AggregateQuery = {
    metrics: [metric],
    group_by: groupBy ? [groupBy] : undefined,
    limit: chart.limit,
    order_by: "metric",
    order: "desc",
  };
  const aggregate = useQuery({
    queryKey: resourceQueryKeys.aggregate(
      resource.id,
      JSON.stringify(aggregateQuery),
    ),
    queryFn: ({ signal }) => resource.api.aggregate(aggregateQuery, { signal }),
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
  const span = Math.min(12, Math.max(1, widget.span || 6));
  const detailPath = `/${resource.slug}?summary=1&report_chart=${encodeURIComponent(chart.name)}`;

  return (
    <article
      className="dashboard-widget"
      style={{ gridColumn: `span ${span}` }}
    >
      <div className="dashboard-widget-header">
        <div>
          <h2>{t(widget.title ?? chart.label)}</h2>
          <span className="muted">{resource.label}</span>
        </div>
        <Link to={detailPath}>{t("View details")}</Link>
      </div>
      {aggregate.isPending && (
        <div className="empty">{t("Loading summary...")}</div>
      )}
      {aggregate.error && (
        <div className="alert" role="alert">
          {errorMessage(aggregate.error)}
        </div>
      )}
      {!aggregate.isPending && !aggregate.error && (!groupBy || chart.type === "kpi") && (
        <div className="insight-value">
          <span>{metricLabel}</span>
          <strong>{formatWidgetValue(aggregateRows[0]?.[metricKey])}</strong>
        </div>
      )}
      {!aggregate.isPending && !aggregate.error && groupBy && chart.type !== "kpi" && (
        <>
          {chartRows && chartRows.length > 0 ? (
            <Suspense
              fallback={<div className="empty">{t("Loading chart...")}</div>}
            >
              <AggregateChart
                type={chart.type}
                rows={chartRows}
                metricLabel={metricLabel}
                groupLabel={groupLabel}
                onSelect={() => {
                  void navigate(detailPath);
                }}
              />
            </Suspense>
          ) : (
            <div className="empty">{t("No summary data")}</div>
          )}
        </>
      )}
    </article>
  );
}

function formatWidgetValue(value: unknown): string {
  if (value === null || value === undefined) return "-";
  return typeof value === "number"
    ? formatNumber(value, { maximumFractionDigits: 4 })
    : String(value);
}
