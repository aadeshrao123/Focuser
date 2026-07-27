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
        title="Schedule"
        description="When this block list is active."
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
            title="No block lists yet"
            description="Create one on the Block Lists page first."
          />
        ) : (
          <>
            <Tabs
              className="mb-5"
              value={mode}
              onChange={setMode}
              items={[
                { id: "always", label: "Always active" },
                { id: "scheduled", label: "On a schedule" },
              ]}
            />

            {/* Editor and summary side by side. The grid alone left most of a
                maximised window empty, and the week was unreadable without
                counting cells. */}
            <div className="grid items-start gap-5 xl:grid-cols-[minmax(0,1fr)_19rem]">
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
          <p className="font-medium text-foreground">Blocking is always on</p>
          <p className="mt-1 text-muted-foreground text-sm leading-relaxed">
            This list applies whenever it is enabled, at any hour of any day. Switch to{" "}
            <span className="text-foreground">On a schedule</span> to limit it to certain hours —
            useful when you only want to be blocked during work, or only in the evenings.
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
        <span className="mr-1 text-muted-foreground text-xs">Presets</span>
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
          Clear
        </Button>
      </div>

      <ScheduleGrid selected={draft} onChange={onChange} />

      <div className="mt-4 flex flex-wrap items-center gap-x-4 gap-y-2 text-faint-foreground text-xs">
        <span className="flex items-center gap-1.5">
          <span className="size-2.5 rounded-[3px] bg-primary" />
          Blocking
        </span>
        <span className="flex items-center gap-1.5">
          <span className="size-2.5 rounded-[3px] bg-elevated" />
          Off
        </span>
        <span className="ml-auto">
          Drag to paint · click a day or hour heading to fill the whole row or column
        </span>
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
      <div className="border-border border-b px-4 py-4">
        <div className="flex items-center justify-between gap-2">
          <span className="flex items-center gap-1.5 text-muted-foreground text-xs">
            <CalendarRange aria-hidden className="size-3.5" />
            Blocking each week
          </span>
          <Badge tone={total > 0 ? "primary" : "neutral"}>{share}%</Badge>
        </div>
        <p className="mt-2 font-semibold text-2xl text-foreground tabular-nums">
          {formatDuration(total * 3600)}
        </p>
        <p className="mt-0.5 text-faint-foreground text-xs">of {WEEK_HOURS} hours in a week</p>
      </div>

      <ul className="divide-y divide-border">
        {DAYS.map((day) => {
          const on = cells ? hoursOn(cells, day) : HOURS.length;
          return (
            <li key={day} className="flex items-baseline justify-between gap-3 px-4 py-2.5">
              <span
                className={cn(
                  "w-9 shrink-0 font-medium text-xs",
                  on > 0 ? "text-foreground" : "text-faint-foreground",
                )}
              >
                {day}
              </span>
              <span
                className={cn(
                  "min-w-0 flex-1 truncate text-right text-xs",
                  on > 0 ? "text-muted-foreground" : "text-faint-foreground",
                )}
                title={cells ? describeDay(cells, day) : "All day"}
              >
                {cells ? describeDay(cells, day) : "All day"}
              </span>
            </li>
          );
        })}
      </ul>

      <div className="flex items-center gap-2 border-border border-t px-4 py-3.5">
        <Button
          onClick={onSave}
          disabled={!dirty || saving}
          icon={dirty ? undefined : <Check />}
          className="flex-1"
        >
          {saving ? "Saving…" : dirty ? "Save schedule" : "Saved"}
        </Button>
        {dirty && (
          <Badge tone="warning" outlined>
            Unsaved
          </Badge>
        )}
      </div>
    </Card>
  );
}
