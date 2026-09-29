import { Lock, Unlock } from "lucide-react";
import { useEffect, useId, useState } from "react";
import type { BlockList, LockSetup } from "@/bindings";
import { Button } from "@/components/ui/button";
import { InlineError } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { NumberField } from "@/components/ui/number-field";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  useEnableProtection,
  useRequestUnlockChallenge,
  useUnlockProtection,
} from "@/lib/commands";
import { formatDuration } from "@/lib/duration";
import { m } from "@/paraglide/messages.js";

export function effectiveLock(list: BlockList) {
  return list.protection && new Date(list.protection.expires_at).getTime() > Date.now()
    ? list.lock
    : (list.scheduled_protection?.lock ?? null);
}

type LockKind = "none" | "password" | "random_text";

const LOCK_KIND_HINTS: Record<LockKind, () => string> = {
  none: m.lists_lock_kind_hint_none,
  password: m.lists_lock_kind_hint_password,
  random_text: m.lists_lock_kind_hint_random_text,
};

export function ProtectForm({ list, onDone }: { list: BlockList; onDone: () => void }) {
  const protect = useEnableProtection();
  const id = useId();
  const passwordId = useId();
  const confirmId = useId();

  const [minutes, setMinutes] = useState(60);
  const [uninstall, setUninstall] = useState(true);
  const [serviceStop, setServiceStop] = useState(true);
  const [modification, setModification] = useState(true);

  const randomTextLengthId = useId();
  const [lockKind, setLockKind] = useState<LockKind>("none");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [randomTextLength, setRandomTextLength] = useState(16);

  const passwordMismatch =
    lockKind === "password" && confirmPassword.length > 0 && password !== confirmPassword;
  const canSubmit =
    lockKind !== "password" || (password.length > 0 && password === confirmPassword);

  function buildLock(): LockSetup | null {
    switch (lockKind) {
      case "password":
        return { kind: "password", password };
      case "random_text":
        return { kind: "random_text", length: randomTextLength };
      case "none":
        return null;
    }
  }

  return (
    <div className="animate-in border-border border-t bg-elevated/40 px-4 py-4 fade-in slide-in-from-top-1">
      <p className="flex items-center gap-2 font-medium text-foreground text-sm">
        <Lock aria-hidden className="size-4 text-warning" />
        {m.lists_lock_heading()}
      </p>
      <p className="mt-1 text-muted-foreground text-sm">{m.lists_lock_warning()}</p>
      <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-3">
        <div className="flex items-center gap-2">
          <label htmlFor={id} className="text-muted-foreground text-sm">
            {m.lists_lock_for()}
          </label>
          <NumberField
            id={id}
            value={minutes}
            onCommit={setMinutes}
            min={1}
            max={10080}
            suffix="min"
          />
        </div>

        <Guard label={m.lists_guard_uninstall()} checked={uninstall} onChange={setUninstall} />
        <Guard label={m.lists_guard_service()} checked={serviceStop} onChange={setServiceStop} />
        <Guard label={m.lists_guard_edit()} checked={modification} onChange={setModification} />
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-3">
        <div className="flex items-center gap-2">
          <span className="text-muted-foreground text-sm">{m.lists_lock_kind_label()}</span>
          <Select
            value={lockKind}
            onValueChange={setLockKind}
            size="sm"
            aria-label={m.lists_lock_kind_label()}
            options={[
              { value: "none", label: m.lists_lock_kind_none() },
              { value: "password", label: m.lists_lock_kind_password() },
              { value: "random_text", label: m.lists_lock_kind_random_text() },
            ]}
          />
        </div>

        {lockKind === "password" && (
          <>
            <Input
              id={passwordId}
              type="password"
              size="sm"
              className="w-40"
              autoComplete="new-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={m.lists_lock_password_placeholder()}
              aria-label={m.lists_lock_password_placeholder()}
            />
            <Input
              id={confirmId}
              type="password"
              size="sm"
              className="w-40"
              autoComplete="new-password"
              invalid={passwordMismatch}
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              placeholder={m.lists_lock_password_confirm_placeholder()}
              aria-label={m.lists_lock_password_confirm_placeholder()}
            />
          </>
        )}

        {lockKind === "random_text" && (
          <div className="flex items-center gap-2">
            <label htmlFor={randomTextLengthId} className="text-muted-foreground text-sm">
              {m.lists_lock_random_text_length()}
            </label>
            <NumberField
              id={randomTextLengthId}
              value={randomTextLength}
              onCommit={setRandomTextLength}
              min={6}
              max={256}
            />
          </div>
        )}
      </div>

      <p className="mt-2 text-faint-foreground text-xs">
        {passwordMismatch ? m.lists_lock_password_mismatch() : LOCK_KIND_HINTS[lockKind]()}
      </p>

      <div className="mt-4 flex items-center gap-2">
        <Button
          size="sm"
          tone="destructive"
          disabled={protect.isPending || !canSubmit}
          onClick={() =>
            protect.mutate(
              {
                listId: list.id,
                minutes,
                preventUninstall: uninstall,
                preventServiceStop: serviceStop,
                preventModification: modification,
                lock: buildLock(),
              },
              { onSuccess: onDone },
            )
          }
        >
          {protect.isPending
            ? m.lists_locking()
            : m.lists_lock_action({ duration: formatDuration(minutes * 60) })}
        </Button>
        <Button variant="ghost" size="sm" onClick={onDone}>
          {m.lists_cancel()}
        </Button>
      </div>

      <InlineError error={protect.error} />
    </div>
  );
}

export function UnlockForm({ list, onDone }: { list: BlockList; onDone: () => void }) {
  const unlock = useUnlockProtection();
  const requestChallenge = useRequestUnlockChallenge();
  const inputId = useId();
  const [response, setResponse] = useState("");

  const lock = effectiveLock(list);
  const isRandomText = lock !== null && "RandomText" in lock;
  const challenge = requestChallenge.data;
  const challengeCharacters = Array.from(challenge ?? "");
  const mismatchIndex =
    isRandomText && challenge
      ? Array.from(response).findIndex(
          (character, index) => character !== challengeCharacters[index],
        )
      : -1;
  const randomTextMatches = !!challenge && !requestChallenge.isPending && response === challenge;
  const mismatchId = `${inputId}-mismatch`;

  // Random text needs a challenge to display before there is anything to
  // type. Asking twice is harmless: the backend returns the one already out.
  useEffect(() => {
    if (isRandomText) {
      requestChallenge.mutate(list.id);
    }
  }, [isRandomText, list.id, requestChallenge.mutate]);

  function submit() {
    if (isRandomText && (!randomTextMatches || unlock.isPending)) return;
    unlock.mutate(
      { listId: list.id, response },
      {
        onSuccess: onDone,
        // A wrong random-text answer consumes the challenge server-side —
        // the old string can never work again, so get a fresh one.
        onError: () => {
          setResponse("");
          if (isRandomText) requestChallenge.mutate(list.id);
        },
      },
    );
  }

  return (
    <div className="animate-in border-border border-t bg-elevated/40 px-4 py-4 fade-in slide-in-from-top-1">
      <p className="flex items-center gap-2 font-medium text-foreground text-sm">
        <Unlock aria-hidden className="size-4 text-warning" />
        {m.lists_unlock_heading({ name: list.name })}
      </p>

      {isRandomText ? (
        <>
          <p className="mt-1 text-muted-foreground text-sm">
            {m.lists_unlock_random_text_instructions()}
          </p>
          {requestChallenge.data && (
            <p className="mt-3 select-all break-all rounded-md border border-border bg-surface px-3 py-2 font-mono text-foreground text-sm tracking-wide">
              <span className="sr-only select-none">{requestChallenge.data}</span>
              <span aria-hidden="true">
                {challengeCharacters.map((character, index) => (
                  <span
                    // biome-ignore lint/suspicious/noArrayIndexKey: Challenge positions are fixed, stateless characters.
                    key={`${index}-${character}`}
                    className={
                      index === mismatchIndex
                        ? "text-destructive"
                        : index < Array.from(response).length &&
                            (mismatchIndex === -1 || index < mismatchIndex)
                          ? "text-success"
                          : undefined
                    }
                  >
                    {character}
                  </span>
                ))}
              </span>
            </p>
          )}
          {requestChallenge.isPending && (
            <p className="mt-3 text-muted-foreground text-sm">
              {m.lists_unlock_random_text_loading()}
            </p>
          )}
          <InlineError error={requestChallenge.error} />
        </>
      ) : null}

      <div className="mt-3 flex items-center gap-2">
        <label htmlFor={inputId} className="sr-only">
          {isRandomText
            ? m.lists_unlock_random_text_placeholder()
            : m.lists_unlock_password_placeholder()}
        </label>
        <Input
          id={inputId}
          type={isRandomText ? "text" : "password"}
          size="sm"
          className="max-w-xs"
          disabled={isRandomText && !requestChallenge.data}
          invalid={mismatchIndex !== -1}
          aria-describedby={mismatchIndex !== -1 ? mismatchId : undefined}
          value={response}
          onChange={(e) => setResponse(e.target.value)}
          placeholder={
            isRandomText
              ? m.lists_unlock_random_text_placeholder()
              : m.lists_unlock_password_placeholder()
          }
          // Friction is the point: typing it is the task, not copying it.
          onPaste={isRandomText ? (e) => e.preventDefault() : undefined}
          onDrop={isRandomText ? (e) => e.preventDefault() : undefined}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              if (isRandomText) e.preventDefault();
              if (response.length > 0) submit();
            }
          }}
        />
      </div>

      {mismatchIndex !== -1 && (
        <p id={mismatchId} role="status" className="mt-2 text-destructive text-sm">
          {m.lists_unlock_random_text_mismatch({ position: mismatchIndex + 1 })}
        </p>
      )}

      <div className="mt-4 flex items-center gap-2">
        <Button
          size="sm"
          tone="destructive"
          disabled={unlock.isPending || (isRandomText ? !randomTextMatches : response.length === 0)}
          onClick={submit}
        >
          {unlock.isPending ? m.lists_unlocking() : m.lists_unlock_action()}
        </Button>
        <Button variant="ghost" size="sm" onClick={onDone}>
          {m.lists_cancel()}
        </Button>
      </div>

      <InlineError error={unlock.error} />
    </div>
  );
}

function Guard({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <Switch size="sm" checked={checked} onCheckedChange={onChange} aria-label={label} />
      <span className="text-muted-foreground text-sm">{label}</span>
    </div>
  );
}
