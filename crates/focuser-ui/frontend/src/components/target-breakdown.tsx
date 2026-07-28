import {
  Area,
  AreaChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { Card } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { assignColors, colorFor, gradientId, SERIES_COLORS } from "@/lib/chart-colors";
import { formatDay } from "@/lib/date-range";
import { formatDuration } from "@/lib/duration";
import type { TargetSeries } from "@/lib/stats";

/** Beyond this the lines stop being followable and fold into "Other". */
const OVERLAY_LIMIT = SERIES_COLORS.length;
const OTHER = "Other";
const OTHER_COLOR = "var(--color-faint-foreground)";

const axisTick = { fill: "var(--color-faint-foreground)", fontSize: 11 };

const tooltipStyle = {
  background: "color-mix(in oklab, var(--color-elevated) 95%, transparent)",
  border: "1px solid var(--color-border-strong)",
  borderRadius: "0.6rem",
  boxShadow: "var(--shadow-depth-md)",
  fontSize: 12,
} as const;

/** Recharts needs one row per day with a column per series. */
function overlayRows(series: TargetSeries[], other: TargetSeries | null) {
  const dates = series[0]?.days.map((d) => d.date) ?? other?.days.map((d) => d.date) ?? [];

  return dates.map((date, i) => {
    // `__date` rather than `date`, so a site actually called "date" cannot
    // collide with the axis key.
    const row: Record<string, string | number> = { __date: date };
    for (const s of series) row[s.target] = s.days[i]?.attempts ?? 0;
    if (other) row[OTHER] = other.days[i]?.attempts ?? 0;
    return row;
  });
}

/** Everything past the top few, added together so the total still adds up. */
function foldRest(rest: TargetSeries[]): TargetSeries | null {
  if (rest.length === 0) return null;
  const first = rest[0];
  if (!first) return null;

  const days = first.days.map((d, i) => ({
    date: d.date,
    attempts: rest.reduce((n, s) => n + (s.days[i]?.attempts ?? 0), 0),
    seconds: rest.reduce((n, s) => n + (s.days[i]?.seconds ?? 0), 0),
  }));

  return {
    target: OTHER,
    attempts: rest.reduce((n, s) => n + s.attempts, 0),
    seconds: rest.reduce((n, s) => n + s.seconds, 0),
    days,
  };
}

/** Every site on one chart, so days can be compared across them. */
export function TargetOverlay({ targets }: { targets: TargetSeries[] }) {
  const shown = targets.slice(0, OVERLAY_LIMIT);
  const other = foldRest(targets.slice(OVERLAY_LIMIT));
  const rows = overlayRows(shown, other);
  const lines = other ? [...shown, other] : shown;
  // Assigned as a set, not one at a time: two lines the same colour on one
  // chart is a bug, whereas two panels sharing one is merely a coincidence.
  const colors = assignColors(shown.map((s) => s.target));
  const colorOf = (target: string) =>
    target === OTHER ? OTHER_COLOR : (colors.get(target) ?? colorFor(target));

  return (
    <Card padding="lg" elevation="raised">
      <div className="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2">
        {lines.map((s) => (
          <span key={s.target} className="flex items-center gap-1.5 text-xs">
            <span
              aria-hidden
              className="size-2.5 rounded-full"
              style={{ backgroundColor: colorOf(s.target) }}
            />
            <span className="text-muted-foreground">{s.target}</span>
            <span className="text-faint-foreground tabular-nums">{s.attempts}</span>
          </span>
        ))}
      </div>

      <div className="h-80 w-full">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={rows} margin={{ top: 8, right: 8, bottom: 0, left: -16 }}>
            <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
            <XAxis
              dataKey="__date"
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
              isAnimationActive={false}
              separator=": "
              labelFormatter={(label) => formatDay(String(label))}
              // Busiest first, so the tooltip reads in the same order the lines
              // are stacked on screen.
              itemSorter={(item) => -Number(item.value ?? 0)}
              contentStyle={tooltipStyle}
              labelStyle={{ color: "var(--color-foreground)" }}
              itemStyle={{ color: "var(--color-muted-foreground)" }}
            />
            {lines.map((s) => (
              <Line
                key={s.target}
                type="monotone"
                dataKey={s.target}
                stroke={colorOf(s.target)}
                strokeWidth={2}
                strokeDasharray={s.target === OTHER ? "4 3" : undefined}
                dot={false}
                activeDot={{ r: 4, strokeWidth: 2, stroke: "var(--color-surface)" }}
                isAnimationActive={false}
              />
            ))}
          </LineChart>
        </ResponsiveContainer>
      </div>
    </Card>
  );
}

/** One site at a time, picked from a list — this is the view that survives
    having hundreds of them. */
export function TargetIndividual({
  targets,
  selected,
  onSelect,
  total,
}: {
  targets: TargetSeries[];
  selected: string;
  onSelect: (target: string) => void;
  total: number;
}) {
  const options = targets.map((t) => ({ value: t.target, label: t.target }));
  const series = targets.find((t) => t.target === selected) ?? targets[0];
  if (!series) return null;

  const color = colorFor(series.target);
  const id = gradientId(series.target);
  const share = total > 0 ? Math.round((series.attempts / total) * 100) : 0;
  const peak = series.days.reduce((n, d) => Math.max(n, d.attempts), 0);

  return (
    <Card padding="lg" elevation="raised">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <Select
            value={series.target}
            onValueChange={onSelect}
            options={options}
            size="sm"
            aria-label="Site or app"
          />
        </div>

        <div className="flex items-end gap-6">
          <Figure label="Blocked" value={series.attempts.toLocaleString()} accent={color} />
          <Figure label="Time" value={formatDuration(series.seconds)} />
          <Figure label="Share" value={`${share}%`} />
          <Figure label="Busiest day" value={String(peak)} />
        </div>
      </div>

      <div className="mt-5 h-72 w-full">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={series.days} margin={{ top: 8, right: 8, bottom: 0, left: -16 }}>
            <defs>
              <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor={color} stopOpacity={0.45} />
                <stop offset="100%" stopColor={color} stopOpacity={0.02} />
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
              isAnimationActive={false}
              separator=": "
              labelFormatter={(label) => formatDay(String(label))}
              formatter={(value) => [value, "Blocked"] as [typeof value, string]}
              contentStyle={tooltipStyle}
              labelStyle={{ color: "var(--color-foreground)" }}
              itemStyle={{ color: "var(--color-muted-foreground)" }}
            />
            <Area
              type="monotone"
              dataKey="attempts"
              stroke={color}
              strokeWidth={2}
              fill={`url(#${id})`}
              isAnimationActive={false}
              activeDot={{ r: 4, fill: color, stroke: "var(--color-surface)", strokeWidth: 2 }}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </Card>
  );
}

function Figure({ label, value, accent }: { label: string; value: string; accent?: string }) {
  return (
    <div className="text-right">
      <p className="text-faint-foreground text-xs">{label}</p>
      <p
        className="mt-1 font-semibold text-lg tabular-nums leading-none"
        style={{ color: accent ?? "var(--color-foreground)" }}
      >
        {value}
      </p>
    </div>
  );
}
