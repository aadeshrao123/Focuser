import { Plus, RotateCcw, Trash2, TriangleAlert } from "lucide-react";
import { type FormEvent, useState } from "react";
import type { AllowanceStatus } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { NumberField } from "@/components/ui/number-field";
import { Progress } from "@/components/ui/progress";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  useAllowances,
  useBrowserStatus,
  useCreateAllowance,
  useDeleteAllowance,
  useResetAllowanceToday,
  useUpdateAllowance,
} from "@/lib/commands";
import { formatDuration } from "@/lib/duration";

const KINDS = [
  { value: "Domain", label: "Website" },
  { value: "AppExecutable", label: "Application" },
] as const;

export function Allowances() {
  const allowances = useAllowances();
  const create = useCreateAllowance();
  const browsers = useBrowserStatus();

  // Browser time is measured by the extension and nothing else, so a website
  // allowance without it would sit at "0s used" forever. Blocking stays on in
  // that case rather than granting an unlimited pass, so say so plainly.
  const extensionConnected = (browsers.data ?? []).some((b) => b.extension_connected);
  const hasWebsiteAllowance = (allowances.data ?? []).some(
    (a) => a.allowance.target.kind === "Domain",
  );

  const [kind, setKind] = useState<(typeof KINDS)[number]["value"]>("Domain");
  const [value, setValue] = useState("");
  const [minutes, setMinutes] = useState(30);
  const [strict, setStrict] = useState(true);

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    const target = value.trim();
    if (!target) return;

    create.mutate(
      { target: { kind, value: target }, dailyLimitSecs: minutes * 60, strictMode: strict },
      { onSuccess: () => setValue("") },
    );
  }

  return (
    <div className="p-8">
      <PageHeader
        title="Allowances"
        description="A daily budget instead of an outright block. When it runs out, the site or app is blocked for the rest of the day."
      />

      {hasWebsiteAllowance && !extensionConnected && (
        <Card className="mb-6 border-warning/40" padding="md">
          <p className="flex items-center gap-2 font-medium text-sm text-warning">
            <TriangleAlert aria-hidden className="size-4" />
            Website allowances need the browser extension
          </p>
          <p className="mt-1 text-muted-foreground text-sm">
            Only the extension can see which tab is open, so without it the timer never starts.
            Those sites stay blocked in the meantime — an allowance that cannot be measured would
            otherwise be an unlimited pass. Install it from Settings.
          </p>
        </Card>
      )}

      <Card className="mb-6" padding="lg">
        <form onSubmit={onSubmit} className="flex flex-wrap items-end gap-3">
          <Labelled label="Type">
            <Select value={kind} onValueChange={setKind} options={KINDS} size="sm" />
          </Labelled>

          <Labelled label={kind === "Domain" ? "Domain" : "Executable"} htmlFor="allowance-target">
            <Input
              id="allowance-target"
              size="sm"
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder={kind === "Domain" ? "youtube.com" : "steam.exe"}
              className="w-56"
            />
          </Labelled>

          <Labelled label="Daily limit" htmlFor="allowance-minutes">
            <NumberField
              id="allowance-minutes"
              value={minutes}
              onCommit={setMinutes}
              min={1}
              max={1440}
              suffix="min"
            />
          </Labelled>

          <Labelled label="Only while focused">
            <Switch checked={strict} onCheckedChange={setStrict} aria-label="Only while focused" />
          </Labelled>

          <Button type="submit" icon={<Plus />} disabled={!value.trim() || create.isPending}>
            {create.isPending ? "Adding…" : "Add"}
          </Button>
        </form>
        <InlineError error={create.error} />
      </Card>

      <QueryState
        isPending={allowances.isPending}
        error={allowances.error}
        onRetry={() => allowances.refetch()}
        isRetrying={allowances.isFetching}
      >
        {allowances.data?.length === 0 ? (
          <EmptyState
            title="No allowances yet"
            description="Add one above to put a daily cap on something instead of blocking it outright."
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {allowances.data?.map((status) => (
              <AllowanceRow key={status.allowance.id} status={status} />
            ))}
          </ul>
        )}
      </QueryState>
    </div>
  );
}

function AllowanceRow({ status }: { status: AllowanceStatus }) {
  const update = useUpdateAllowance();
  const reset = useResetAllowanceToday();
  const remove = useDeleteAllowance();

  const { allowance: a, used_today_secs: used, remaining_secs: left, exhausted } = status;
  const ratio = a.daily_limit_secs > 0 ? used / a.daily_limit_secs : 0;

  // Only the field being edited changes; the rest is sent back as-is.
  const save = (
    patch: Partial<{ dailyLimitSecs: number; strictMode: boolean; enabled: boolean }>,
  ) =>
    update.mutate({
      id: a.id,
      dailyLimitSecs: a.daily_limit_secs,
      strictMode: a.strict_mode,
      enabled: a.enabled,
      ...patch,
    });

  return (
    <li>
      <Card>
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <p className="truncate font-medium text-foreground text-sm">{a.target.value}</p>
            <p className="text-faint-foreground text-xs">
              {a.target.kind === "Domain" ? "Website" : "Application"}
              {a.strict_mode && " · counted only while focused"}
              {!a.enabled && " · paused"}
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            <NumberField
              value={Math.round(a.daily_limit_secs / 60)}
              onCommit={(m) => save({ dailyLimitSecs: m * 60 })}
              min={1}
              max={1440}
              suffix="min"
              aria-describedby={`allowance-${a.id}-usage`}
            />
            <Switch
              checked={a.enabled}
              onCheckedChange={(enabled) => save({ enabled })}
              aria-label={`Enable allowance for ${a.target.value}`}
            />
            <Button
              variant="ghost"
              size="icon"
              aria-label={`Reset today's usage for ${a.target.value}`}
              onClick={() => reset.mutate(a.id)}
            >
              <RotateCcw />
            </Button>
            <Button
              variant="ghost"
              tone="destructive"
              size="icon"
              aria-label={`Delete allowance for ${a.target.value}`}
              onClick={() => remove.mutate(a.id)}
            >
              <Trash2 />
            </Button>
          </div>
        </div>

        <Progress
          className="mt-3"
          value={ratio}
          tone={exhausted ? "destructive" : ratio > 0.75 ? "warning" : "success"}
          label={`Usage for ${a.target.value}`}
        />
        <p id={`allowance-${a.id}-usage`} className="mt-1.5 text-faint-foreground text-xs">
          {exhausted
            ? `Used all ${formatDuration(a.daily_limit_secs)} — blocked for the rest of today`
            : `${formatDuration(used)} used · ${formatDuration(left)} left`}
        </p>

        <InlineError error={update.error ?? reset.error ?? remove.error} />
      </Card>
    </li>
  );
}

function Labelled({
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
