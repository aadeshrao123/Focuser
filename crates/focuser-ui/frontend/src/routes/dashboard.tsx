import { Ban, ListChecks, Lock, ShieldCheck } from "lucide-react";
import { FocusSession } from "@/components/focus-session";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { QueryState } from "@/components/ui/feedback";
import { Stat, StatGrid } from "@/components/ui/stat";
import { Switch } from "@/components/ui/switch";
import { useBlockLists, useProtectionStatus, useStats, useToggleBlockList } from "@/lib/commands";
import { rangeFor } from "@/lib/date-range";
import { formatDuration } from "@/lib/duration";
import { count } from "@/lib/utils";

export function Dashboard() {
  const lists = useBlockLists();
  const protection = useProtectionStatus();
  const toggle = useToggleBlockList();

  const today = rangeFor("today");
  const stats = useStats(today.from, today.to);

  const all = lists.data ?? [];
  const enabled = all.filter((l) => l.enabled);
  const rules = enabled.reduce((n, l) => n + l.websites.length + l.applications.length, 0);
  const blockedToday = (stats.data ?? []).reduce((n, s) => n + s.blocked_attempts, 0);
  const locked = protection.data ?? [];
  const soonest = locked.reduce<number | null>(
    (min, p) => (min === null || p.remaining_seconds < min ? p.remaining_seconds : min),
    null,
  );

  return (
    <div className="p-8">
      <PageHeader title="Dashboard" description="Where things stand right now." />

      <QueryState
        isPending={lists.isPending}
        error={lists.error}
        onRetry={() => lists.refetch()}
        isRetrying={lists.isFetching}
      >
        <StatGrid>
          <Stat
            label="Blocked today"
            value={blockedToday.toLocaleString()}
            icon={<Ban aria-hidden className="size-3.5" />}
            tone="primary"
          />
          <Stat
            label="Lists enabled"
            value={`${enabled.length} of ${all.length}`}
            icon={<ListChecks aria-hidden className="size-3.5" />}
          />
          <Stat
            label="Rules in force"
            value={rules}
            hint="Sites and apps across enabled lists"
            icon={<ShieldCheck aria-hidden className="size-3.5" />}
          />
          <Stat
            label="Protected"
            value={locked.length}
            hint={soonest !== null ? `Unlocks in ${formatDuration(soonest)}` : undefined}
            icon={<Lock aria-hidden className="size-3.5" />}
            tone={locked.length > 0 ? "warning" : "default"}
          />
        </StatGrid>

        <div className="mt-6">
          <FocusSession lists={all} />
        </div>

        <section className="mt-6">
          <h2 className="mb-3 font-medium text-foreground text-sm">Block lists</h2>
          {all.length === 0 ? (
            <EmptyState
              title="No block lists yet"
              description="Create one on the Block Lists page to start blocking."
            />
          ) : (
            <Card className="divide-y divide-border" padding="none">
              {all.map((list) => {
                const isLocked = locked.some((p) => p.block_list_id === list.id);
                return (
                  <div key={list.id} className="flex items-center justify-between gap-4 px-4 py-3">
                    <div className="min-w-0">
                      <p className="truncate font-medium text-foreground text-sm">{list.name}</p>
                      <p className="text-faint-foreground text-xs">
                        {count(list.websites.length, "site")} ·{" "}
                        {count(list.applications.length, "app")}
                        {list.schedule && " · on a schedule"}
                        {isLocked && " · protected"}
                      </p>
                    </div>
                    <Switch
                      checked={list.enabled}
                      onCheckedChange={(next) => toggle.mutate({ id: list.id, enabled: next })}
                      disabled={isLocked}
                      aria-label={`Enable ${list.name}`}
                    />
                  </div>
                );
              })}
            </Card>
          )}
        </section>
      </QueryState>
    </div>
  );
}
