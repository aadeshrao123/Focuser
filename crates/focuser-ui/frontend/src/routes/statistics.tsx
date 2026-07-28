import { Ban, BarChart3, CalendarDays, Clock, Globe } from "lucide-react";
import { useMemo, useState } from "react";
import { TargetIndividual, TargetOverlay } from "@/components/target-breakdown";
import { Badge } from "@/components/ui/badge";
import { Card, EmptyState, PageHeader, Section } from "@/components/ui/card";
import { ConfirmButton } from "@/components/ui/confirm-button";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Page } from "@/components/ui/page";
import { Select } from "@/components/ui/select";
import { Stat, StatGrid } from "@/components/ui/stat";
import { Tabs } from "@/components/ui/tabs";
import { UsageChart } from "@/components/usage-chart";
import { useClearStatistics, useStats } from "@/lib/commands";
import { formatDay, RANGES, type RangeId, rangeFor } from "@/lib/date-range";
import { formatDuration } from "@/lib/duration";
import { seriesByTarget, summarise, totalsByDay, totalsByTarget } from "@/lib/stats";
import { count } from "@/lib/utils";

const TOP_LIMIT = 10;

export function Statistics() {
  const [rangeId, setRangeId] = useState<RangeId>("7d");
  const [view, setView] = useState<"bars" | "individual" | "detailed">("bars");
  // Empty until the user picks; the chart falls back to the busiest target.
  const [picked, setPicked] = useState("");
  const clear = useClearStatistics();

  // Rebuilt only when the preset changes, so a poll at midnight can't shift the
  // range under a query key that is already in flight.
  const range = useMemo(() => rangeFor(rangeId), [rangeId]);
  const stats = useStats(range.from, range.to);

  const rows = stats.data ?? [];
  const days = useMemo(() => totalsByDay(rows, range), [rows, range]);
  const targets = useMemo(() => totalsByTarget(rows), [rows]);
  const series = useMemo(() => seriesByTarget(rows, range), [rows, range]);
  const totals = summarise(days, targets);

  return (
    <Page wide>
      <PageHeader
        title="Statistics"
        description="What Focuser has been keeping you away from."
        actions={
          <>
            <Select
              value={rangeId}
              onValueChange={setRangeId}
              options={RANGES}
              size="sm"
              aria-label="Date range"
            />
            <ConfirmButton
              variant="outline"
              size="sm"
              onConfirm={() => clear.mutate()}
              disabled={clear.isPending}
            >
              Clear history
            </ConfirmButton>
          </>
        }
      />

      <QueryState
        isPending={stats.isPending}
        error={stats.error}
        onRetry={() => stats.refetch()}
        isRetrying={stats.isFetching}
      >
        <StatGrid>
          <Stat
            label="Blocked attempts"
            value={totals.attempts.toLocaleString()}
            icon={<Ban aria-hidden className="size-3.5" />}
            tone="primary"
          />
          <Stat
            label="Time recorded"
            value={formatDuration(totals.seconds)}
            icon={<Clock aria-hidden className="size-3.5" />}
          />
          <Stat
            label="Sites and apps"
            value={totals.targets}
            icon={<Globe aria-hidden className="size-3.5" />}
          />
          <Stat
            label="Busiest day"
            value={totals.busiestDay ? formatDay(totals.busiestDay.date) : "—"}
            hint={totals.busiestDay ? `${totals.busiestDay.attempts} blocked attempts` : undefined}
            icon={<CalendarDays aria-hidden className="size-3.5" />}
          />
        </StatGrid>

        <Section
          title="Blocked attempts per day"
          actions={
            <Tabs
              value={view}
              onChange={setView}
              items={[
                { id: "bars", label: "Bars" },
                { id: "individual", label: "Individual" },
                { id: "detailed", label: "Detailed" },
              ]}
            />
          }
        >
          {view === "bars" ? (
            <Card padding="lg" elevation="raised">
              <UsageChart data={days} />
            </Card>
          ) : series.length === 0 ? (
            <EmptyState
              icon={<BarChart3 />}
              title="Nothing recorded yet"
              description="Sites and apps get their own charts here once something is blocked."
            />
          ) : view === "individual" ? (
            <TargetIndividual
              targets={series}
              selected={picked}
              onSelect={setPicked}
              total={totals.attempts}
            />
          ) : (
            <TargetOverlay targets={series} />
          )}
        </Section>

        <Section
          title="Most blocked"
          actions={
            targets.length > 0 ? (
              <Badge tone="neutral">{count(targets.length, "site or app", "sites and apps")}</Badge>
            ) : undefined
          }
        >
          {targets.length === 0 ? (
            <EmptyState
              icon={<BarChart3 />}
              title="Nothing recorded yet"
              description="Numbers appear here once a block list starts turning things away."
            />
          ) : (
            <Card padding="none" elevation="raised" className="overflow-hidden">
              <table className="w-full text-sm">
                <caption className="sr-only">
                  Sites and apps by blocked attempts, busiest first
                </caption>
                <thead>
                  <tr className="border-border border-b bg-elevated/40 text-muted-foreground text-xs">
                    <th scope="col" className="px-4 py-2 text-left font-normal">
                      Site or app
                    </th>
                    <th scope="col" className="px-4 py-2">
                      <span className="sr-only">Share</span>
                    </th>
                    <th scope="col" className="px-4 py-2 text-right font-normal">
                      Attempts
                    </th>
                    <th scope="col" className="px-4 py-2 text-right font-normal">
                      Time
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {targets.slice(0, TOP_LIMIT).map((t, rank) => (
                    <tr
                      key={t.target}
                      className="border-border/60 border-b transition-colors last:border-0 hover:bg-elevated/50"
                    >
                      <td className="px-4 py-2.5">
                        <div className="flex items-center gap-3">
                          <span className="w-5 shrink-0 text-right font-medium text-faint-foreground text-xs tabular-nums">
                            {rank + 1}
                          </span>
                          <span className="truncate text-foreground">{t.target}</span>
                        </div>
                      </td>
                      <td className="px-4 py-2.5">
                        {/* Bar relative to the busiest target, so the gap between
                            first and fifth is visible without reading numbers. */}
                        <div
                          aria-hidden
                          className="ml-auto h-1.5 w-24 overflow-hidden rounded-full bg-elevated"
                        >
                          <div
                            className="h-full rounded-full bg-primary/70"
                            style={{
                              width: `${Math.max(4, (t.attempts / (targets[0]?.attempts || 1)) * 100)}%`,
                            }}
                          />
                        </div>
                      </td>
                      <td className="px-4 py-2.5 text-right font-medium tabular-nums">
                        {t.attempts}
                      </td>
                      <td className="px-4 py-2.5 text-right text-muted-foreground tabular-nums">
                        {formatDuration(t.seconds)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Card>
          )}
          {targets.length > TOP_LIMIT && (
            <p className="mt-2 text-faint-foreground text-xs">
              Showing the top {TOP_LIMIT} of {targets.length}.
            </p>
          )}
        </Section>

        <InlineError error={clear.error} />
      </QueryState>
    </Page>
  );
}
