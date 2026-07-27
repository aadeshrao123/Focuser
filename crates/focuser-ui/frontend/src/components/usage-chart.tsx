import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { formatDay } from "@/lib/date-range";
import type { DayTotal } from "@/lib/stats";

const axisTick = { fill: "var(--color-faint-foreground)", fontSize: 11 };

const tooltipStyle = {
  background: "color-mix(in oklab, var(--color-elevated) 95%, transparent)",
  border: "1px solid var(--color-border-strong)",
  borderRadius: "0.6rem",
  boxShadow: "var(--shadow-depth-md)",
  fontSize: 12,
} as const;

/** Blocked attempts per day. Days with nothing recorded still take up space. */
export function UsageChart({ data }: { data: DayTotal[] }) {
  return (
    <div className="h-64 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: -16 }}>
          <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
          <XAxis
            dataKey="date"
            tickFormatter={formatDay}
            tick={axisTick}
            tickLine={false}
            axisLine={false}
            minTickGap={16}
          />
          <YAxis
            allowDecimals={false}
            width={40}
            tick={axisTick}
            tickLine={false}
            axisLine={false}
          />
          <Tooltip
            // No cursor band: it painted a full-height rectangle over the whole
            // day, which read as a second, taller bar. `activeBar` below lights
            // up the one bar being pointed at instead.
            cursor={false}
            labelFormatter={(label) => formatDay(String(label))}
            formatter={(value) => [value, "Blocked attempts"] as [typeof value, string]}
            contentStyle={tooltipStyle}
            labelStyle={{ color: "var(--color-foreground)" }}
            itemStyle={{ color: "var(--color-muted-foreground)" }}
          />
          {/* No grow-in animation: the page polls every couple of seconds, and
              replaying it on each refetch is noise. It also keeps the chart
              deterministic for the browser tests. */}
          <defs>
            {/* A vertical fade rather than a flat fill: the bar reads as lit
                from above, matching the shadows everywhere else. */}
            <linearGradient id="bar-primary" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--color-primary-hover)" stopOpacity={1} />
              <stop offset="100%" stopColor="var(--color-primary)" stopOpacity={0.55} />
            </linearGradient>
            {/* Same ramp, brighter and fully opaque, so the hovered bar lifts
                out of the row without changing size or position. */}
            <linearGradient id="bar-primary-active" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--color-primary-active)" stopOpacity={1} />
              <stop offset="100%" stopColor="var(--color-primary-hover)" stopOpacity={0.95} />
            </linearGradient>
          </defs>
          <Bar
            dataKey="attempts"
            fill="url(#bar-primary)"
            activeBar={{ fill: "url(#bar-primary-active)" }}
            radius={[4, 4, 2, 2]}
            maxBarSize={44}
            isAnimationActive={false}
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

/** Same data as a filled trend line, for when the shape matters more than the day. */
export function UsageTrend({ data }: { data: DayTotal[] }) {
  return (
    <div className="h-56 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: -16 }}>
          <defs>
            <linearGradient id="area-primary" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--color-primary)" stopOpacity={0.45} />
              <stop offset="100%" stopColor="var(--color-primary)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
          <XAxis
            dataKey="date"
            tickFormatter={formatDay}
            tick={axisTick}
            tickLine={false}
            axisLine={false}
            minTickGap={16}
          />
          <YAxis
            allowDecimals={false}
            width={40}
            tick={axisTick}
            tickLine={false}
            axisLine={false}
          />
          <Tooltip
            cursor={{ stroke: "var(--color-border-strong)" }}
            labelFormatter={(label) => formatDay(String(label))}
            formatter={(value) => [value, "Blocked attempts"] as [typeof value, string]}
            contentStyle={tooltipStyle}
            labelStyle={{ color: "var(--color-foreground)" }}
            itemStyle={{ color: "var(--color-muted-foreground)" }}
          />
          <Area
            type="monotone"
            dataKey="attempts"
            stroke="var(--color-primary)"
            strokeWidth={2}
            fill="url(#area-primary)"
            isAnimationActive={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
