import { Ban, ListChecks, ListPlus, Lock, ShieldCheck } from "lucide-react";
import { Link } from "react-router-dom";
import { FocusSession } from "@/components/focus-session";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader, Section } from "@/components/ui/card";
import { InlineError } from "@/components/ui/feedback";
import { Page } from "@/components/ui/page";
import { ListSkeleton, StatGridSkeleton } from "@/components/ui/skeleton";
import { Stat, StatGrid } from "@/components/ui/stat";
import { Switch } from "@/components/ui/switch";
import { Tooltip } from "@/components/ui/tooltip";
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
    <Page wide>
      <PageHeader title="Dashboard" description="Where things stand right now." />

      {lists.isPending ? (
        <StatGridSkeleton />
      ) : (
        <StatGrid>
          <Stat
            label="Blocked today"
            value={blockedToday.toLocaleString()}
            icon={<Ban aria-hidden />}
            tone="primary"
            hint={blockedToday === 0 ? "Nothing turned away yet" : "Attempts stopped"}
          />
          <Stat
            label="Lists enabled"
            value={`${enabled.length} of ${all.length}`}
            icon={<ListChecks aria-hidden />}
            footer={<EnabledBar enabled={enabled.length} total={all.length} />}
          />
          <Stat
            label="Rules in force"
            value={rules.toLocaleString()}
            icon={<ShieldCheck aria-hidden />}
            hint="Sites and apps across enabled lists"
          />
          <Stat
            label="Protected"
            value={locked.length}
            icon={<Lock aria-hidden />}
            tone={locked.length > 0 ? "warning" : "default"}
            hint={soonest !== null ? `Unlocks in ${formatDuration(soonest)}` : "No locks running"}
          />
        </StatGrid>
      )}

      <div className="mt-7">
        <FocusSession lists={all} />
      </div>

      <Section
        title="Block lists"
        description="Turn a list on or off without leaving this page."
        actions={
          <Button asChild variant="outline" size="sm">
            <Link to="/block-lists">
              <ListPlus aria-hidden className="size-4" />
              Manage
            </Link>
          </Button>
        }
      >
        {lists.isPending ? (
          <ListSkeleton rows={2} />
        ) : all.length === 0 ? (
          <EmptyState
            icon={<ListChecks />}
            title="No block lists yet"
            description="A block list groups the sites and apps you want out of the way. Create one to start."
            action={
              <Button asChild>
                <Link to="/block-lists">
                  <ListPlus aria-hidden className="size-4" />
                  Create a block list
                </Link>
              </Button>
            }
          />
        ) : (
          <div className="flex flex-col gap-2">
            {all.map((list) => {
              const lock = locked.find((p) => p.block_list_id === list.id);
              return (
                <Card
                  key={list.id}
                  padding="none"
                  elevation="raised"
                  className="flex items-center justify-between gap-4 px-4 py-3.5 transition-colors hover:border-border-strong"
                >
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <p className="truncate font-medium text-foreground text-sm">{list.name}</p>
                      {lock && (
                        <Badge tone="warning" icon={<Lock aria-hidden />}>
                          {formatDuration(lock.remaining_seconds)}
                        </Badge>
                      )}
                      {list.schedule && <Badge tone="info">Scheduled</Badge>}
                    </div>
                    <p className="mt-0.5 text-faint-foreground text-xs">
                      {count(list.websites.length, "site")} ·{" "}
                      {count(list.applications.length, "app")}
                    </p>
                  </div>

                  {lock ? (
                    <Tooltip content="Locked until the protection window expires">
                      <span>
                        <Switch
                          checked={list.enabled}
                          onCheckedChange={() => {}}
                          disabled
                          aria-label={`Enable ${list.name}`}
                        />
                      </span>
                    </Tooltip>
                  ) : (
                    <Switch
                      checked={list.enabled}
                      onCheckedChange={(next) => toggle.mutate({ id: list.id, enabled: next })}
                      aria-label={`Enable ${list.name}`}
                    />
                  )}
                </Card>
              );
            })}
          </div>
        )}
        <InlineError error={lists.error ?? toggle.error} />
      </Section>
    </Page>
  );
}

/** How much of the collection is switched on, at a glance. */
function EnabledBar({ enabled, total }: { enabled: number; total: number }) {
  if (total === 0) return null;

  return (
    <div className="flex gap-1" aria-hidden>
      {Array.from({ length: total }, (_, i) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: positions, not entities
          key={i}
          className={`h-1 flex-1 rounded-full ${i < enabled ? "bg-primary" : "bg-elevated"}`}
        />
      ))}
    </div>
  );
}
