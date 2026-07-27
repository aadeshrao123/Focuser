import { Infinity as InfinityIcon } from "lucide-react";
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
import { type CellKey, cellsToSlots, PRESETS, slotsToCells } from "@/lib/schedule";

const sameCells = (a: Set<CellKey>, b: Set<CellKey>) =>
  a.size === b.size && [...a].every((k) => b.has(k));

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

            {mode === "always" ? (
              <Card padding="lg" elevation="raised" className="flex items-center gap-4">
                <span className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-primary/15 text-primary ring-1 ring-primary/25 ring-inset">
                  <InfinityIcon aria-hidden className="size-5" />
                </span>
                <div>
                  <p className="font-medium text-foreground text-sm">Blocking is always on</p>
                  <p className="mt-0.5 text-muted-foreground text-sm">
                    This list applies whenever it is enabled, at any hour of any day.
                  </p>
                </div>
              </Card>
            ) : (
              <Card padding="lg" elevation="raised" className="edge-light">
                <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="mr-1 text-muted-foreground text-xs">Presets</span>
                    {PRESETS.map((p) => (
                      <Button
                        key={p.id}
                        variant="outline"
                        size="sm"
                        onClick={() => setDraft(p.build())}
                      >
                        {p.label}
                      </Button>
                    ))}
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setDraft(new Set())}
                      disabled={draft.size === 0}
                    >
                      Clear
                    </Button>
                  </div>

                  <Badge tone={draft.size > 0 ? "primary" : "neutral"} size="md">
                    {formatDuration(draft.size * 3600)} a week
                  </Badge>
                </div>

                <ScheduleGrid selected={draft} onChange={setDraft} />

                <div className="mt-4 flex items-center gap-4 text-faint-foreground text-xs">
                  <span className="flex items-center gap-1.5">
                    <span className="size-2.5 rounded-[3px] bg-primary" />
                    Blocking
                  </span>
                  <span className="flex items-center gap-1.5">
                    <span className="size-2.5 rounded-[3px] bg-elevated" />
                    Off
                  </span>
                  <span className="ml-auto">Click or drag to paint hours</span>
                </div>
              </Card>
            )}

            <div className="mt-5 flex items-center gap-3">
              <Button onClick={commit} disabled={!dirty || save.isPending}>
                {save.isPending ? "Saving…" : "Save schedule"}
              </Button>
              {dirty && (
                <Badge tone="warning" size="md" outlined>
                  Unsaved changes
                </Badge>
              )}
            </div>
            <InlineError error={save.error} />
          </>
        )}
      </QueryState>
    </Page>
  );
}
