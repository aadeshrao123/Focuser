import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { formatDay } from "@/lib/date-range";
import type { DayTotal } from "@/lib/stats";

const axisTick = { fill: "var(--color-faint-foreground)", fontSize: 11 };

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
            cursor={{ fill: "var(--color-hover)" }}
            labelFormatter={(label) => formatDay(String(label))}
            formatter={(value) => [value, "Blocked attempts"] as [typeof value, string]}
            contentStyle={{
              background: "var(--color-elevated)",
              border: "1px solid var(--color-border-strong)",
              borderRadius: "0.5rem",
              fontSize: 12,
            }}
            labelStyle={{ color: "var(--color-foreground)" }}
            itemStyle={{ color: "var(--color-muted-foreground)" }}
          />
          {/* No grow-in animation: the page polls every couple of seconds, and
              replaying it on each refetch is noise. It also keeps the chart
              deterministic for the browser tests. */}
          <Bar
            dataKey="attempts"
            fill="var(--color-primary)"
            radius={[3, 3, 0, 0]}
            maxBarSize={48}
            isAnimationActive={false}
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
