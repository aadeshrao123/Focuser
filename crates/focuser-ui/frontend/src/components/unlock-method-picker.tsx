import { Hourglass, Keyboard, KeyRound } from "lucide-react";
import { type ReactNode, useId } from "react";
import type { Lock, LockSetup } from "@/bindings";
import { Input } from "@/components/ui/input";
import { NumberField } from "@/components/ui/number-field";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages.js";

type Kind = "random_text" | "password" | "none";

/**
 * What a lock form holds while the user chooses. Every field is kept whatever
 * the kind, so switching away from a half-typed password and back loses nothing.
 */
export interface UnlockMethod {
  kind: Kind;
  length: number;
  password: string;
  confirm: string;
}

const DEFAULT_LENGTH = 16;

/**
 * The starting point for a form. `undefined` is a lock that does not exist yet,
 * `null` is a saved lock with no early unlock.
 *
 * A saved password comes back as a hash, so there is nothing to show: the
 * fields start empty and the form is not ready until it is typed again.
 */
export function methodFromLock(lock: Lock | null | undefined): UnlockMethod {
  const blank = { length: DEFAULT_LENGTH, password: "", confirm: "" };
  if (lock === undefined) return { ...blank, kind: "random_text" };
  if (lock === null) return { ...blank, kind: "none" };
  if (lock.RandomText) return { ...blank, kind: "random_text", length: lock.RandomText.length };
  return { ...blank, kind: "password" };
}

/** The shape the command core takes. No lock means no early unlock. */
export function toLockSetup(method: UnlockMethod): LockSetup | null {
  switch (method.kind) {
    case "random_text":
      return { kind: "random_text", length: method.length };
    case "password":
      return { kind: "password", password: method.password };
    case "none":
      return null;
  }
}

/** A password has to be there, and typed the same twice. */
export function methodReady(method: UnlockMethod): boolean {
  return (
    method.kind !== "password" ||
    (method.password.trim().length > 0 && method.password === method.confirm)
  );
}

const HINTS: Record<Kind, () => string> = {
  random_text: m.lists_lock_kind_hint_random_text,
  password: m.lists_lock_kind_hint_password,
  none: m.lists_lock_kind_hint_none,
};

/**
 * "How can you unlock early?", for a manual lock and a scheduled one alike.
 *
 * Three choices laid out in the open rather than folded into a select: there
 * are only three, each one changes what the lock means, and a Radix select
 * cannot open inside a modal dialog anyway (see `Dialog`).
 */
export function UnlockMethodPicker({
  value,
  onChange,
  disabled,
}: {
  value: UnlockMethod;
  onChange: (next: UnlockMethod) => void;
  disabled?: boolean;
}) {
  const name = useId();
  const mismatch = value.kind === "password" && value.confirm.length > 0 && !methodReady(value);

  const option = (kind: Kind, icon: ReactNode, label: string, extra?: ReactNode) => (
    <div
      className={cn(
        "flex min-h-12 items-center gap-3 rounded-lg border px-3 py-1.5 transition-colors",
        "has-focus-visible:outline-2 has-focus-visible:outline-offset-2 has-focus-visible:outline-ring",
        value.kind === kind
          ? "border-primary/60 bg-primary-dim"
          : "border-border hover:border-border-strong",
        disabled && "opacity-50",
      )}
    >
      <label
        className={cn(
          "flex flex-1 items-center gap-3 text-foreground text-sm",
          disabled ? "cursor-not-allowed" : "cursor-pointer",
        )}
      >
        <input
          type="radio"
          className="sr-only"
          name={name}
          checked={value.kind === kind}
          disabled={disabled}
          onChange={() => onChange({ ...value, kind })}
        />
        <span aria-hidden className="text-muted-foreground [&_svg]:size-4">
          {icon}
        </span>
        {label}
      </label>
      {value.kind === kind && extra}
    </div>
  );

  return (
    <fieldset className="min-w-0">
      <legend className="text-foreground text-sm">{m.lock_method_question()}</legend>

      <div className="mt-2 flex flex-col gap-1.5">
        {option(
          "random_text",
          <Keyboard />,
          m.lists_lock_kind_random_text(),
          <span className="flex items-center gap-2 text-muted-foreground text-xs">
            <NumberField
              value={value.length}
              onCommit={(length) => onChange({ ...value, length })}
              min={6}
              max={5000}
              disabled={disabled}
              aria-label={m.lists_lock_random_text_length()}
            />
            {m.lock_length_unit()}
          </span>,
        )}
        {option("password", <KeyRound />, m.lists_lock_kind_password())}
        {option("none", <Hourglass />, m.lists_lock_kind_none())}
      </div>

      {value.kind === "password" && (
        <div className="mt-2 flex gap-2">
          <Input
            type="password"
            size="sm"
            autoComplete="new-password"
            disabled={disabled}
            value={value.password}
            onChange={(e) => onChange({ ...value, password: e.target.value })}
            placeholder={m.lists_lock_password_placeholder()}
            aria-label={m.lists_lock_password_placeholder()}
          />
          <Input
            type="password"
            size="sm"
            autoComplete="new-password"
            disabled={disabled}
            invalid={mismatch}
            value={value.confirm}
            onChange={(e) => onChange({ ...value, confirm: e.target.value })}
            placeholder={m.lists_lock_password_confirm_placeholder()}
            aria-label={m.lists_lock_password_confirm_placeholder()}
          />
        </div>
      )}

      <p className="mt-2 text-faint-foreground text-xs">
        {mismatch ? m.lists_lock_password_mismatch() : HINTS[value.kind]()}
      </p>
    </fieldset>
  );
}
