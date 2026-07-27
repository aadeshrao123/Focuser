import { Pause, Play, SkipForward, Square } from "lucide-react";
import { useState } from "react";
import type { BlockList } from "@/bindings";
import { ListPicker, resolveSelected } from "@/components/list-picker";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { InlineError } from "@/components/ui/feedback";
import { NumberField } from "@/components/ui/number-field";
import { Progress } from "@/components/ui/progress";
import {
  usePausePomodoro,
  usePomodoroStatus,
  useResumePomodoro,
  useSkipPomodoro,
  useStartPomodoro,
  useStopPomodoro,
} from "@/lib/commands";
import { formatCountdown } from "@/lib/duration";

const PHASE_LABEL = {
  work: "Focus",
  short_break: "Short break",
  long_break: "Long break",
} as const;

// Same defaults as `focuser pomodoro start`.
const DEFAULTS = { work: 25, shortBreak: 5, longBreak: 15, cycles: 4 };

export function FocusSession({ lists }: { lists: BlockList[] }) {
  const status = usePomodoroStatus();

  return (
    <Card padding="lg">
      <h2 className="font-medium text-foreground text-sm">Focus session</h2>
      {status.data ? <Running status={status.data} /> : <StartForm lists={lists} />}
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
      <div className="flex items-baseline justify-between gap-4">
        <div className="min-w-0">
          <p className="font-semibold text-3xl text-foreground tabular-nums tracking-tight">
            {formatCountdown(status.remaining_secs)}
          </p>
          <p className="mt-1 truncate text-muted-foreground text-sm">
            {PHASE_LABEL[status.current_phase]} · {status.block_list_name} · cycle{" "}
            {status.current_cycle}
            {status.paused && " · paused"}
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

function StartForm({ lists }: { lists: BlockList[] }) {
  const start = useStartPomodoro();
  const [rawSelected, setSelected] = useState("");
  const [work, setWork] = useState(DEFAULTS.work);
  const [shortBreak, setShortBreak] = useState(DEFAULTS.shortBreak);
  const [longBreak, setLongBreak] = useState(DEFAULTS.longBreak);
  const [cycles, setCycles] = useState(DEFAULTS.cycles);

  const selected = resolveSelected(lists, rawSelected);

  if (lists.length === 0) {
    return (
      <p className="mt-3 text-muted-foreground text-sm">
        Create a block list first — a session needs something to block.
      </p>
    );
  }

  return (
    <>
      <p className="mt-1 text-muted-foreground text-sm">
        Turns a block list on while you work and off during breaks.
      </p>

      <div className="mt-4 flex flex-wrap items-end gap-4">
        <ListPicker lists={lists} value={selected} onChange={setSelected} />
        <Field label="Focus" htmlFor="pomo-work">
          <NumberField
            id="pomo-work"
            value={work}
            onCommit={setWork}
            min={1}
            max={480}
            suffix="min"
          />
        </Field>
        <Field label="Short break" htmlFor="pomo-short">
          <NumberField
            id="pomo-short"
            value={shortBreak}
            onCommit={setShortBreak}
            min={1}
            max={120}
            suffix="min"
          />
        </Field>
        <Field label="Long break" htmlFor="pomo-long">
          <NumberField
            id="pomo-long"
            value={longBreak}
            onCommit={setLongBreak}
            min={1}
            max={120}
            suffix="min"
          />
        </Field>
        <Field label="Cycles" htmlFor="pomo-cycles">
          <NumberField id="pomo-cycles" value={cycles} onCommit={setCycles} min={1} max={20} />
        </Field>

        <Button
          icon={<Play />}
          disabled={!selected || start.isPending}
          onClick={() =>
            start.mutate({
              blockListId: selected,
              config: {
                work_secs: work * 60,
                short_break_secs: shortBreak * 60,
                long_break_secs: longBreak * 60,
                cycles_until_long_break: cycles,
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
