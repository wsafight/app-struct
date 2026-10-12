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
  "var(--color-accent)",
  "#547da1",
  "#bd965a",
  "#9d80ad",
  "#76996f",
  "#b86d76",
];

const tooltipStyle = {
  background: "var(--color-surface)",
  border: "1px solid var(--color-border)",
  borderRadius: 10,
  boxShadow: "0 8px 24px rgb(27 47 34 / 10%)",
  fontSize: 12,
};

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
  onSelect,
}: {
  type: ChartType;
  rows: ChartRow[];
  metricLabel: string;
  groupLabel: string;
  onSelect?(index: number): void;
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
              onClick={(_, index) => onSelect?.(index)}
            >
              {rows.map((row, index) => (
                <Cell
                  key={`${row.dimension}-${index}`}
                  fill={chartColors[index % chartColors.length]}
                />
              ))}
            </Pie>
            <Tooltip contentStyle={tooltipStyle} />
            <Legend
              iconType="circle"
              iconSize={8}
              wrapperStyle={{ fontSize: 12, paddingTop: 12 }}
            />
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
              <XAxis
                type="number"
                axisLine={false}
                tickLine={false}
                tick={{ fontSize: 12 }}
              />
              <YAxis
                dataKey="dimension"
                type="category"
                width={140}
                tick={{ fontSize: 12 }}
                axisLine={false}
                tickLine={false}
              />
            </>
          ) : (
            <>
              <XAxis
                dataKey="dimension"
                type="category"
                tick={{ fontSize: 12 }}
                axisLine={false}
                tickLine={false}
              />
              <YAxis
                type="number"
                axisLine={false}
                tickLine={false}
                tick={{ fontSize: 12 }}
              />
            </>
          )}
          <Tooltip
            contentStyle={tooltipStyle}
            cursor={{ fill: "var(--color-surface-soft)" }}
          />
          <Bar
            dataKey="value"
            name={metricLabel}
            fill="var(--color-accent)"
            radius={horizontal ? [0, 5, 5, 0] : [5, 5, 0, 0]}
            isAnimationActive={false}
            onClick={(_, index) => onSelect?.(index)}
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
