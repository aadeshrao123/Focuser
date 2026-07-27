import { useEffect, useState } from "react";
import { Input } from "./input";
import { cn } from "@/lib/utils";

export interface NumberFieldProps {
  value: number;
  onCommit: (value: number) => void;
  min?: number;
  max?: number;
  suffix?: string;
  disabled?: boolean;
  id?: string;
  "aria-describedby"?: string;
  className?: string;
}

/**
 * Number input that saves on blur or Enter rather than on every keystroke —
 * typing "30" would otherwise write 3 first. Out-of-range or unparseable input
 * snaps back to the last good value.
 */
export function NumberField({
  value,
  onCommit,
  min,
  max,
  suffix,
  disabled,
  className,
  ...props
}: NumberFieldProps) {
  const [draft, setDraft] = useState(String(value));

  useEffect(() => setDraft(String(value)), [value]);

  function commit() {
    const parsed = Number(draft);
    const valid =
      draft.trim() !== "" &&
      Number.isFinite(parsed) &&
      (min === undefined || parsed >= min) &&
      (max === undefined || parsed <= max);

    if (!valid) {
      setDraft(String(value));
      return;
    }
    if (parsed !== value) onCommit(parsed);
  }

  return (
    <div className={cn("flex items-center gap-2", className)}>
      <Input
        {...props}
        type="number"
        inputMode="numeric"
        size="sm"
        min={min}
        max={max}
        disabled={disabled}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setDraft(String(value));
        }}
        className="w-24"
      />
      {suffix && <span className="text-muted-foreground text-sm">{suffix}</span>}
    </div>
  );
}
