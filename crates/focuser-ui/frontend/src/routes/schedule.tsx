import { useEffect, useMemo, useState } from "react";
import { ListPicker, resolveSelected } from "@/components/list-picker";
import { ScheduleGrid } from "@/components/schedule-grid";
import { Button } from "@/components/ui/button";
import { EmptyState, PageHeader } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { useBlockLists, useUpdateSchedule } from "@/lib/commands";
import { type CellKey, cellsToSlots, PRESETS, slotsToCells } from "@/lib/schedule";
import { cn } from "@/lib/utils";

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
    <div className="p-8">
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
            <div className="mb-5 flex gap-2">
              <ModeButton active={mode === "always"} onClick={() => setMode("always")}>
                Always active
              </ModeButton>
              <ModeButton active={mode === "scheduled"} onClick={() => setMode("scheduled")}>
                On a schedule
              </ModeButton>
            </div>

            {mode === "always" ? (
              <p className="mb-6 text-muted-foreground text-sm">
                Blocking applies whenever this list is enabled.
              </p>
            ) : (
              <>
                <div className="mb-3 flex flex-wrap items-center gap-2">
                  <span className="text-muted-foreground text-sm">Presets</span>
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

                <ScheduleGrid selected={draft} onChange={setDraft} />

                <p className="mt-3 text-faint-foreground text-xs">
                  Click or drag to paint hours. {draft.size} of 168 selected.
                </p>
              </>
            )}

            <div className="mt-6 flex items-center gap-3">
              <Button onClick={commit} disabled={!dirty || save.isPending}>
                {save.isPending ? "Saving…" : "Save schedule"}
              </Button>
              {dirty && <span className="text-muted-foreground text-sm">Unsaved changes</span>}
            </div>
            <InlineError error={save.error} />
          </>
        )}
      </QueryState>
    </div>
  );
}

function ModeButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: string;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "rounded-md border px-3 py-1.5 text-sm transition-colors",
        active
          ? "border-primary bg-primary-dim text-foreground"
          : "border-border text-muted-foreground hover:bg-hover hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}
