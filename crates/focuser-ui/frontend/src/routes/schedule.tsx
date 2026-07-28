import {
  Briefcase,
  CalendarRange,
  Check,
  Eraser,
  Infinity as InfinityIcon,
  Moon,
  Sunrise,
} from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import { ListPicker, resolveSelected } from "@/components/list-picker";
import { ScheduleGrid } from "@/components/schedule-grid";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Page } from "@/components/ui/page";
import { Tabs } from "@/components/ui/tabs";
import { useBlockLists, useUpdateSchedule } from "@/lib/commands";
import { formatDuration } from "@/lib/duration";
import {
  type CellKey,
  cellsToSlots,
  DAYS,
  describeDay,
  HOURS,
  hoursOn,
  PRESETS,
  slotsToCells,
} from "@/lib/schedule";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages.js";

const sameCells = (a: Set<CellKey>, b: Set<CellKey>) =>
  a.size === b.size && [...a].every((k) => b.has(k));

const WEEK_HOURS = DAYS.length * HOURS.length;

/** Presets are defined in the lib; the icon is presentation, so it lives here. */
const PRESET_ICONS: Record<string, ReactNode> = {
  work: <Briefcase />,
  evenings: <Moon />,
  weekends: <Sunrise />,
  always: <InfinityIcon />,
};

export function Schedule() {
  const lists = useBlockLists();
  const save = useUpdateSchedule();
  const [rawSelected, setSelected] = useState("");

  const all = lists.data ?? [];
  const selected = resolveSelected(all, rawSelected);
  const list = all.find((l) => l.id === selected);

  const saved = useMemo(() => slotsToCells(list?.schedule?.time_slots ?? []), [list?.schedule]);
  const alwaysActive = list ? !list.schedule : true;

  const [draft, setDraft] = useState<Set<CellKey>>(saved);
  const [mode, setMode] = useState<"always" | "scheduled">("always");

  // Reset the draft whenever we switch lists or the saved schedule changes.
  useEffect(() => {
    setDraft(saved);
    setMode(alwaysActive ? "always" : "scheduled");
  }, [saved, alwaysActive]);

  const dirty =
    list != null && (mode === "always" ? !alwaysActive : alwaysActive || !sameCells(draft, saved));

  function commit() {
    if (!list) return;
    save.mutate({
      listId: list.id,
      slots: mode === "always" ? [] : cellsToSlots(draft),
      alwaysActive: mode === "always",
    });
  }

  return (
    <Page wide>
      <PageHeader
        title={m.schedule_title()}
        description={m.schedule_description()}
        actions={<ListPicker lists={all} value={selected} onChange={setSelected} />}
      />

      <QueryState
        isPending={lists.isPending}
        error={lists.error}
        onRetry={() => lists.refetch()}
        isRetrying={lists.isFetching}
      >
        {!list ? (
          <EmptyState
            title={m.websites_no_lists_title()}
            description={m.websites_no_lists_description()}
          />
        ) : (
          <>
            <Tabs
              className="mb-5"
              value={mode}
              onChange={setMode}
              items={[
                { id: "always", label: m.schedule_mode_always() },
                { id: "scheduled", label: m.schedule_mode_scheduled() },
              ]}
            />

            {/* Editor first, summary under it. Side by side squeezed the grid
                into a column too narrow for 24 legible hours. */}
            <div className="space-y-5">
              {mode === "always" ? (
                <AlwaysPanel />
              ) : (
                <GridPanel draft={draft} onChange={setDraft} />
              )}

              <WeekSummary
                cells={mode === "always" ? null : draft}
                dirty={dirty}
                saving={save.isPending}
                onSave={commit}
              />
            </div>

            <InlineError error={save.error} />
          </>
        )}
      </QueryState>
    </Page>
  );
}

function AlwaysPanel() {
  return (
    <Card padding="lg" elevation="raised" className="edge-light">
      <div className="flex items-start gap-4">
        <span className="flex size-12 shrink-0 items-center justify-center rounded-xl bg-primary/15 text-primary ring-1 ring-primary/25 ring-inset">
          <InfinityIcon aria-hidden className="size-6" />
        </span>
        <div className="min-w-0">
          <p className="font-medium text-foreground">{m.schedule_always_heading()}</p>
          <p className="mt-1 text-muted-foreground text-sm leading-relaxed">
            {m.schedule_always_body_before()}{" "}
            <span className="text-foreground">{m.schedule_mode_scheduled()}</span>{" "}
            {m.schedule_always_body_after()}
          </p>
        </div>
      </div>

      {/* A full week at a glance, so this tab is not a single sentence on an
          otherwise blank page. */}
      <div className="mt-6 grid grid-cols-7 gap-1.5">
        {DAYS.map((day) => (
          <div
            key={day}
            className="rounded-lg bg-primary/15 px-2 py-3 text-center ring-1 ring-primary/20 ring-inset"
          >
            <p className="font-medium text-primary text-xs">{day}</p>
            <p className="mt-0.5 text-[10px] text-primary/70">24h</p>
          </div>
        ))}
      </div>
    </Card>
  );
}

function GridPanel({
  draft,
  onChange,
}: {
  draft: Set<CellKey>;
  onChange: React.Dispatch<React.SetStateAction<Set<CellKey>>>;
}) {
  return (
    <Card padding="lg" elevation="raised" className="edge-light">
      <div className="mb-4 flex flex-wrap items-center gap-1.5">
        <span className="mr-1 text-muted-foreground text-xs">{m.grid_presets()}</span>
        {PRESETS.map((p) => (
          <Button
            key={p.id}
            variant="outline"
            size="sm"
            icon={PRESET_ICONS[p.id]}
            onClick={() => onChange(p.build())}
          >
            {p.label}
          </Button>
        ))}
        <Button
          variant="ghost"
          size="sm"
          icon={<Eraser />}
          onClick={() => onChange(new Set())}
          disabled={draft.size === 0}
          className="ml-auto"
        >
          {m.common_clear()}
        </Button>
      </div>

      <ScheduleGrid selected={draft} onChange={onChange} />

      <div className="mt-4 flex flex-wrap items-center gap-x-4 gap-y-2 text-faint-foreground text-xs">
        <span className="flex items-center gap-1.5">
          <span className="size-2.5 rounded-[3px] bg-primary" />
          {m.grid_legend_blocking()}
        </span>
        <span className="flex items-center gap-1.5">
          <span className="size-2.5 rounded-[3px] bg-elevated" />
          {m.grid_legend_off()}
        </span>
        <span className="ml-auto">{m.grid_hint()}</span>
      </div>
    </Card>
  );
}

/** The week in words, plus the save action. `null` cells means "always on". */
function WeekSummary({
  cells,
  dirty,
  saving,
  onSave,
}: {
  cells: Set<CellKey> | null;
  dirty: boolean;
  saving: boolean;
  onSave: () => void;
}) {
  const total = cells ? cells.size : WEEK_HOURS;
  const share = Math.round((total / WEEK_HOURS) * 100);

  return (
    <Card padding="none" elevation="raised" className="edge-light overflow-hidden">
      <div className="flex flex-wrap items-end gap-x-8 gap-y-4 px-5 py-4">
        <div>
          <span className="flex items-center gap-1.5 text-muted-foreground text-xs">
            <CalendarRange aria-hidden className="size-3.5" />
            {m.schedule_weekly_total()}
          </span>
          <p className="mt-1.5 flex items-baseline gap-2.5">
            <span className="font-semibold text-3xl text-foreground tabular-nums">
              {formatDuration(total * 3600)}
            </span>
            <span className="text-faint-foreground text-xs">of {WEEK_HOURS} hours</span>
          </p>
        </div>

        <div className="min-w-48 flex-1">
          <div className="mb-2 flex items-center justify-between gap-2">
            <span className="text-faint-foreground text-xs">{m.schedule_coverage()}</span>
            <Badge tone={total > 0 ? "primary" : "neutral"}>{share}%</Badge>
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-elevated">
            <div
              className="h-full rounded-full bg-primary transition-[width] duration-300"
              style={{ width: `${share}%` }}
            />
          </div>
        </div>

        <div className="ml-auto flex items-center gap-2">
          {dirty && (
            <Badge tone="warning" outlined>
              {m.schedule_unsaved()}
            </Badge>
          )}
          <Button onClick={onSave} disabled={!dirty || saving} icon={dirty ? undefined : <Check />}>
            {saving ? m.schedule_saving() : dirty ? m.schedule_save() : m.schedule_saved()}
          </Button>
        </div>
      </div>

      {/* One column per day, so the week reads left to right like the grid
          above it rather than as a list running the other way. */}
      <div className="grid grid-cols-2 gap-px border-border border-t bg-border sm:grid-cols-4 lg:grid-cols-7">
        {DAYS.map((day) => {
          const on = cells ? hoursOn(cells, day) : HOURS.length;
          const text = cells ? describeDay(cells, day) : m.schedule_all_day();
          return (
            <div key={day} className="bg-surface px-4 py-3">
              <div className="flex items-baseline justify-between gap-2">
                <span
                  className={cn(
                    "font-medium text-sm",
                    on > 0 ? "text-foreground" : "text-faint-foreground",
                  )}
                >
                  {day}
                </span>
                <span
                  className={cn(
                    "text-xs tabular-nums",
                    on > 0 ? "text-primary" : "text-faint-foreground",
                  )}
                >
                  {on > 0 ? `${on}h` : "off"}
                </span>
              </div>
              <p
                className={cn(
                  "mt-1 truncate text-xs",
                  on > 0 ? "text-muted-foreground" : "text-faint-foreground",
                )}
                title={text}
              >
                {text}
              </p>
            </div>
          );
        })}
      </div>
    </Card>
  );
}
