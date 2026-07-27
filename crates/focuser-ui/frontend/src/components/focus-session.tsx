import { Pause, Play, SkipForward, Square, Timer } from "lucide-react";
import { useState } from "react";
import type { BlockList, PomodoroConfig } from "@/bindings";
import { ListPicker, resolveSelected } from "@/components/list-picker";
import { LiveBadge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { InlineError } from "@/components/ui/feedback";
import { NumberField } from "@/components/ui/number-field";
import { Progress } from "@/components/ui/progress";
import {
  usePausePomodoro,
  usePomodoroPresets,
  usePomodoroStatus,
  useResumePomodoro,
  useSkipPomodoro,
  useStartPomodoro,
  useStopPomodoro,
} from "@/lib/commands";
import { formatCountdown } from "@/lib/duration";
import { cn } from "@/lib/utils";

const PHASE_LABEL = {
  work: "Focus",
  short_break: "Short break",
  long_break: "Long break",
} as const;

export function FocusSession({ lists }: { lists: BlockList[] }) {
  const status = usePomodoroStatus();
  const running = status.data;

  return (
    <Card
      padding="lg"
      elevation="raised"
      className={cn(
        "edge-light relative overflow-hidden transition-colors",
        // A running session earns a tinted border and a wash; idle stays quiet
        // so it does not compete with the numbers above it.
        running && "border-primary/30",
      )}
    >
      {running && (
        <div
          aria-hidden
          className="pointer-events-none absolute inset-x-0 -top-24 h-48 bg-primary/10 blur-3xl"
        />
      )}

      <div className="relative">
        <div className="flex items-center justify-between gap-3">
          <h2 className="flex items-center gap-2 font-medium text-foreground text-sm">
            <Timer aria-hidden className="size-4 text-primary" />
            Focus session
          </h2>
          {running && (
            <LiveBadge tone={running.current_phase === "work" ? "primary" : "success"}>
              {running.paused ? "Paused" : PHASE_LABEL[running.current_phase]}
            </LiveBadge>
          )}
        </div>

        {running ? <Running status={running} /> : <StartForm lists={lists} />}
      </div>
    </Card>
  );
}

function Running({
  status,
}: {
  status: NonNullable<ReturnType<typeof usePomodoroStatus>["data"]>;
}) {
  const pause = usePausePomodoro();
  const resume = useResumePomodoro();
  const skip = useSkipPomodoro();
  const stop = useStopPomodoro();

  const elapsed = status.phase_duration_secs - status.remaining_secs;
  const progress = status.phase_duration_secs > 0 ? elapsed / status.phase_duration_secs : 0;

  return (
    <div className="mt-4">
      <div className="flex items-end justify-between gap-4">
        <div className="min-w-0">
          <p className="font-semibold text-5xl text-foreground leading-none tabular-nums tracking-tight">
            {formatCountdown(status.remaining_secs)}
          </p>
          <p className="mt-2.5 truncate text-muted-foreground text-sm">
            {status.block_list_name} · cycle {status.current_cycle} of{" "}
            {status.config.cycles_until_long_break}
          </p>
        </div>

        <div className="flex shrink-0 gap-2">
          {status.paused ? (
            <Button size="sm" icon={<Play />} onClick={() => resume.mutate()}>
              Resume
            </Button>
          ) : (
            <Button variant="outline" size="sm" icon={<Pause />} onClick={() => pause.mutate()}>
              Pause
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            icon={<SkipForward />}
            onClick={() => skip.mutate()}
            aria-label="Skip to the next phase"
          >
            Skip
          </Button>
          <Button
            variant="ghost"
            tone="destructive"
            size="sm"
            icon={<Square />}
            onClick={() => stop.mutate()}
          >
            Stop
          </Button>
        </div>
      </div>

      <Progress className="mt-4" value={progress} label="Phase progress" />

      <InlineError error={pause.error ?? resume.error ?? skip.error ?? stop.error} />
    </div>
  );
}

type Minutes = { work: number; shortBreak: number; longBreak: number; cycles: number };

function toMinutes(config: PomodoroConfig): Minutes {
  return {
    work: Math.round(config.work_secs / 60),
    shortBreak: Math.round(config.short_break_secs / 60),
    longBreak: Math.round(config.long_break_secs / 60),
    cycles: config.cycles_until_long_break,
  };
}

function sameMinutes(a: Minutes, b: Minutes) {
  return (
    a.work === b.work &&
    a.shortBreak === b.shortBreak &&
    a.longBreak === b.longBreak &&
    a.cycles === b.cycles
  );
}

function StartForm({ lists }: { lists: BlockList[] }) {
  const start = useStartPomodoro();
  const presets = usePomodoroPresets();
  const [rawSelected, setSelected] = useState("");
  // Null until the user touches something. The durations shown come from the
  // first preset, so the numbers live in Rust and are never restated here.
  const [draft, setDraft] = useState<Minutes | null>(null);

  const selected = resolveSelected(lists, rawSelected);

  if (lists.length === 0) {
    return (
      <p className="mt-3 text-muted-foreground text-sm">
        Create a block list first — a session needs something to block.
      </p>
    );
  }

  if (!presets.data) {
    return (
      <>
        <p className="mt-3 text-muted-foreground text-sm">Loading presets…</p>
        <InlineError error={presets.error} />
      </>
    );
  }

  const values = draft ?? toMinutes(presets.data[0].config);
  // Derived, not stored: editing a field naturally clears the highlight
  // because the values stop matching any preset.
  const activeKey = presets.data.find((p) => sameMinutes(toMinutes(p.config), values))?.key;

  const set = (patch: Partial<Minutes>) => setDraft({ ...values, ...patch });

  return (
    <>
      <p className="mt-1 text-muted-foreground text-sm">
        Turns a block list on while you work and off during breaks.
      </p>

      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        {presets.data.map((preset) => {
          const active = preset.key === activeKey;
          return (
            <button
              key={preset.key}
              type="button"
              aria-pressed={active}
              onClick={() => setDraft(toMinutes(preset.config))}
              className={cn(
                "rounded-full border px-3 py-1 text-xs transition-colors",
                active
                  ? "border-primary/40 bg-primary/15 text-foreground"
                  : "border-border text-muted-foreground hover:bg-hover hover:text-foreground",
              )}
            >
              {preset.label}
            </button>
          );
        })}
      </div>

      <div className="mt-4 flex flex-wrap items-end gap-4">
        <ListPicker lists={lists} value={selected} onChange={setSelected} />
        <Field label="Focus" htmlFor="pomo-work">
          <NumberField
            id="pomo-work"
            value={values.work}
            onCommit={(work) => set({ work })}
            min={1}
            max={480}
            suffix="min"
          />
        </Field>
        <Field label="Short break" htmlFor="pomo-short">
          <NumberField
            id="pomo-short"
            value={values.shortBreak}
            onCommit={(shortBreak) => set({ shortBreak })}
            min={1}
            max={120}
            suffix="min"
          />
        </Field>
        <Field label="Long break" htmlFor="pomo-long">
          <NumberField
            id="pomo-long"
            value={values.longBreak}
            onCommit={(longBreak) => set({ longBreak })}
            min={1}
            max={120}
            suffix="min"
          />
        </Field>
        <Field label="Cycles" htmlFor="pomo-cycles">
          <NumberField
            id="pomo-cycles"
            value={values.cycles}
            onCommit={(cycles) => set({ cycles })}
            min={1}
            max={20}
          />
        </Field>

        <Button
          icon={<Play />}
          disabled={!selected || start.isPending}
          onClick={() =>
            start.mutate({
              blockListId: selected,
              config: {
                work_secs: values.work * 60,
                short_break_secs: values.shortBreak * 60,
                long_break_secs: values.longBreak * 60,
                cycles_until_long_break: values.cycles,
              },
            })
          }
        >
          {start.isPending ? "Starting…" : "Start"}
        </Button>
      </div>

      <InlineError error={start.error} />
    </>
  );
}

function Field({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label htmlFor={htmlFor} className="mb-1.5 block text-muted-foreground text-xs">
        {label}
      </label>
      {children}
    </div>
  );
}
