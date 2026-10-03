import { Lock, Unlock } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import type { BlockList, Protection } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Notice } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { NumberField } from "@/components/ui/number-field";
import { Switch } from "@/components/ui/switch";
import {
  methodFromLock,
  methodReady,
  toLockSetup,
  UnlockMethodPicker,
} from "@/components/unlock-method-picker";
import {
  useEnableProtection,
  useRequestUnlockChallenge,
  useUnlockProtection,
} from "@/lib/commands";
import { formatDuration } from "@/lib/duration";
import { m } from "@/paraglide/messages.js";

/** Whether a protection is still running. One with no end runs until unlocked. */
export function protectionActive(protection: Protection | null | undefined): boolean {
  if (!protection) return false;
  return protection.expires_at === null || new Date(protection.expires_at).getTime() > Date.now();
}

export function effectiveLock(list: BlockList) {
  return protectionActive(list.protection) ? list.lock : (list.scheduled_protection?.lock ?? null);
}

interface LockDialogProps {
  list: BlockList;
  open: boolean;
  onClose: () => void;
}

/** Lock a list on for a set time, whatever its schedule says. */
export function ProtectDialog({ list, open, onClose }: LockDialogProps) {
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={m.lists_lock_dialog_title({ name: list.name })}
      icon={<Lock aria-hidden className="text-warning" />}
    >
      <ProtectForm list={list} onDone={onClose} />
    </Dialog>
  );
}

/** End the lock that is running on a list, by its password or its random text. */
export function UnlockDialog({ list, open, onClose }: LockDialogProps) {
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={m.lists_unlock_heading({ name: list.name })}
      icon={<Unlock aria-hidden className="text-warning" />}
    >
      <UnlockForm list={list} onDone={onClose} />
    </Dialog>
  );
}

export function ProtectForm({ list, onDone }: { list: BlockList; onDone: () => void }) {
  const protect = useEnableProtection();
  const id = useId();

  const [minutes, setMinutes] = useState(60);
  const [untilUnlocked, setUntilUnlocked] = useState(false);
  const [uninstall, setUninstall] = useState(true);
  const [serviceStop, setServiceStop] = useState(true);
  const [modification, setModification] = useState(true);
  // A timed lock has always started with no way out. That is what "lock" meant
  // before there were unlock methods, so choosing one stays a deliberate step.
  const [method, setMethod] = useState(() => methodFromLock(null));
  // With no timer the unlock method is the only way out, so one is required.
  const ready = methodReady(method) && (!untilUnlocked || method.kind !== "none");

  return (
    <>
      <div className="mt-4 flex items-center gap-2">
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
          disabled={untilUnlocked}
        />
      </div>
      <div className="mt-2">
        <Guard
          label={m.lists_lock_until_unlocked()}
          checked={untilUnlocked}
          onChange={setUntilUnlocked}
        />
        {untilUnlocked && (
          <p className="mt-1 text-muted-foreground text-xs">{m.lists_lock_until_unlocked_hint()}</p>
        )}
      </div>

      <div className="mt-3 flex flex-col gap-2">
        <Guard label={m.lists_guard_uninstall()} checked={uninstall} onChange={setUninstall} />
        <Guard label={m.lists_guard_service()} checked={serviceStop} onChange={setServiceStop} />
        <Guard label={m.lists_guard_edit()} checked={modification} onChange={setModification} />
      </div>

      <div className="mt-4">
        <UnlockMethodPicker value={method} onChange={setMethod} disabled={protect.isPending} />
      </div>

      <Notice error={protect.error} />

      <div className="mt-5 flex items-center justify-end gap-2">
        <Button variant="ghost" size="sm" onClick={onDone}>
          {m.lists_cancel()}
        </Button>
        <Button
          size="sm"
          disabled={protect.isPending || !ready}
          onClick={() =>
            protect.mutate(
              {
                listId: list.id,
                minutes: untilUnlocked ? null : minutes,
                preventUninstall: uninstall,
                preventServiceStop: serviceStop,
                preventModification: modification,
                lock: toLockSetup(method),
              },
              { onSuccess: onDone },
            )
          }
        >
          {protect.isPending
            ? m.lists_locking()
            : untilUnlocked
              ? m.lists_lock_action_until_unlocked()
              : m.lists_lock_action({ duration: formatDuration(minutes * 60) })}
        </Button>
      </div>
    </>
  );
}

export function UnlockForm({ list, onDone }: { list: BlockList; onDone: () => void }) {
  const unlock = useUnlockProtection();
  const requestChallenge = useRequestUnlockChallenge();
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
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

  // Typing is all this form is for. A password field can take the cursor as the
  // pop-up opens; a random-text one is disabled until its text arrives.
  useEffect(() => {
    if (challenge) inputRef.current?.focus();
  }, [challenge]);

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
    <>
      {isRandomText ? (
        <>
          <p className="mt-2 text-muted-foreground text-sm">
            {m.lists_unlock_random_text_instructions()}
          </p>
          {requestChallenge.data && (
            <p className="mt-3 max-h-60 select-all overflow-y-auto break-all rounded-md border border-border bg-surface px-3 py-2 font-mono text-base text-foreground tracking-wide">
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
          <Notice error={requestChallenge.error} />
        </>
      ) : null}

      <div className="mt-3">
        <label htmlFor={inputId} className="sr-only">
          {isRandomText
            ? m.lists_unlock_random_text_placeholder()
            : m.lists_unlock_password_placeholder()}
        </label>
        <Input
          id={inputId}
          ref={inputRef}
          data-autofocus
          type={isRandomText ? "text" : "password"}
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
        <p id={mismatchId} role="status" className="mt-2 text-warning text-xs">
          {m.lists_unlock_random_text_mismatch({ position: mismatchIndex + 1 })}
        </p>
      )}

      <Notice error={unlock.error} />

      <div className="mt-5 flex items-center justify-end gap-2">
        <Button variant="ghost" size="sm" onClick={onDone}>
          {m.lists_cancel()}
        </Button>
        <Button
          size="sm"
          disabled={unlock.isPending || (isRandomText ? !randomTextMatches : response.length === 0)}
          onClick={submit}
        >
          {unlock.isPending ? m.lists_unlocking() : m.lists_unlock_action()}
        </Button>
      </div>
    </>
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
