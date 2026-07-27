import { ChevronDown, ChevronUp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

export interface NumberFieldProps {
  value: number;
  onCommit: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  suffix?: string;
  disabled?: boolean;
  id?: string;
  "aria-describedby"?: string;
  "aria-label"?: string;
  className?: string;
}

/**
 * Number input that saves on blur or Enter rather than on every keystroke —
 * typing "30" would otherwise write 3 first. Out-of-range or unparseable input
 * snaps back to the last good value.
 *
 * The native spinner is hidden and replaced with our own: browsers draw it as
 * two hairline arrows that only appear on hover, at a size nobody can hit.
 */
export function NumberField({
  value,
  onCommit,
  min,
  max,
  step = 1,
  suffix,
  disabled,
  className,
  ...props
}: NumberFieldProps) {
  const [draft, setDraft] = useState(String(value));

  /**
   * The number on screen, tracked outside React state.
   *
   * Two things need it. Most callers save through a mutation, so `value` keeps
   * reporting the old number until the write lands — left alone, that stale
   * prop resets the draft and swallows the step. And two presses in the same
   * frame both read the same `draft` from their closure, so the second is lost.
   * A ref updates synchronously and survives both.
   */
  const shown = useRef(value);
  /** What we last sent and are still waiting to see echoed back. */
  const pending = useRef<number | null>(null);

  useEffect(() => {
    if (pending.current !== null) {
      if (pending.current !== value) return;
      pending.current = null;
    }
    shown.current = value;
    setDraft(String(value));
  }, [value]);

  const clamp = (n: number) => Math.min(max ?? Number.MAX_SAFE_INTEGER, Math.max(min ?? 0, n));

  function set(next: number) {
    shown.current = next;
    setDraft(String(next));
    if (next === value) return;
    pending.current = next;
    onCommit(next);
  }

  function commit() {
    const parsed = Number(draft);
    const valid =
      draft.trim() !== "" &&
      Number.isFinite(parsed) &&
      (min === undefined || parsed >= min) &&
      (max === undefined || parsed <= max);

    set(valid ? parsed : shown.current);
  }

  // Stepping commits straight away — there is nothing half-typed to protect.
  function nudge(delta: number) {
    const next = clamp(shown.current + delta);
    if (next !== shown.current) set(next);
  }

  const atMin = min !== undefined && shown.current <= min;
  const atMax = max !== undefined && shown.current >= max;

  return (
    <div
      className={cn(
        "inline-flex h-9 items-stretch overflow-hidden rounded-lg border border-border bg-surface",
        "transition-colors focus-within:border-primary/60 hover:border-border-strong",
        "focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-ring",
        disabled && "cursor-not-allowed opacity-50",
        className,
      )}
    >
      <input
        {...props}
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setDraft(String(shown.current));
        }}
        className={cn(
          "w-14 bg-transparent pl-3 text-right font-medium text-foreground text-sm tabular-nums",
          "outline-none disabled:cursor-not-allowed",
          // Chromium draws its own spinner on top of ours otherwise.
          "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
        )}
      />

      {suffix && (
        <span className="flex select-none items-center pr-1 pl-1.5 text-muted-foreground text-xs">
          {suffix}
        </span>
      )}

      <div className="flex w-7 flex-col border-border border-l">
        <Step direction="up" onClick={() => nudge(step)} disabled={disabled || atMax} />
        <span className="h-px bg-border" />
        <Step direction="down" onClick={() => nudge(-step)} disabled={disabled || atMin} />
      </div>
    </div>
  );
}

function Step({
  direction,
  onClick,
  disabled,
}: {
  direction: "up" | "down";
  onClick: () => void;
  disabled?: boolean;
}) {
  const Icon = direction === "up" ? ChevronUp : ChevronDown;
  return (
    <button
      type="button"
      // The input owns the accessible name and value; these are a shortcut for
      // the mouse, and arrow keys already do the same job from the keyboard.
      tabIndex={-1}
      aria-hidden
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex flex-1 items-center justify-center text-muted-foreground transition-colors",
        "hover:bg-hover hover:text-foreground active:bg-active",
        "disabled:pointer-events-none disabled:opacity-30",
      )}
    >
      <Icon className="size-3" strokeWidth={2.5} />
    </button>
  );
}
