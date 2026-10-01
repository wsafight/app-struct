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
import type { ChartType } from "../../resource";

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

/**
 * Bar and donut rendering for resource summaries.
 *
 * Split into its own module so `recharts` is only fetched when a summary panel is
 * actually expanded; the resource list loads without it.
 */
export function AggregateChart({
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
