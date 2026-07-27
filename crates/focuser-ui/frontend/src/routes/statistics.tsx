import { Ban, CalendarDays, Clock, Globe } from "lucide-react";
import { useMemo, useState } from "react";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { ConfirmButton } from "@/components/ui/confirm-button";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Select } from "@/components/ui/select";
import { Stat, StatGrid } from "@/components/ui/stat";
import { UsageChart } from "@/components/usage-chart";
import { useClearStatistics, useStats } from "@/lib/commands";
import { formatDay, RANGES, type RangeId, rangeFor } from "@/lib/date-range";
import { formatDuration, summarise, totalsByDay, totalsByTarget } from "@/lib/stats";

const TOP_LIMIT = 10;

export function Statistics() {
  const [rangeId, setRangeId] = useState<RangeId>("7d");
  const clear = useClearStatistics();

  // Rebuilt only when the preset changes, so a poll at midnight can't shift the
  // range under a query key that is already in flight.
  const range = useMemo(() => rangeFor(rangeId), [rangeId]);
  const stats = useStats(range.from, range.to);

  const rows = stats.data ?? [];
  const days = useMemo(() => totalsByDay(rows, range), [rows, range]);
  const targets = useMemo(() => totalsByTarget(rows), [rows]);
  const totals = summarise(days, targets);

  return (
    <div className="p-8">
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

        <Card className="mt-6" padding="lg">
          <h2 className="mb-4 font-medium text-foreground text-sm">Blocked attempts per day</h2>
          <UsageChart data={days} />
        </Card>

        <section className="mt-6">
          <h2 className="mb-3 font-medium text-foreground text-sm">Most blocked</h2>
          {targets.length === 0 ? (
            <EmptyState
              title="Nothing recorded yet"
              description="Numbers appear here once a block list starts turning things away."
            />
          ) : (
            <Card padding="none">
              <table className="w-full text-sm">
                <caption className="sr-only">
                  Sites and apps by blocked attempts, busiest first
                </caption>
                <thead>
                  <tr className="border-border border-b text-muted-foreground text-xs">
                    <th scope="col" className="px-4 py-2 text-left font-normal">
                      Site or app
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
                  {targets.slice(0, TOP_LIMIT).map((t) => (
                    <tr key={t.target} className="border-border/60 border-b last:border-0">
                      <td className="truncate px-4 py-2.5 text-foreground">{t.target}</td>
                      <td className="px-4 py-2.5 text-right tabular-nums">{t.attempts}</td>
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
        </section>

        <InlineError error={clear.error} />
      </QueryState>
    </div>
  );
}
