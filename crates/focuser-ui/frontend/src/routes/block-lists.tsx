import { ListChecks, Lock, Plus, Trash2, Unlock } from "lucide-react";
import { type FormEvent, useEffect, useId, useState } from "react";
import type { BlockList, LockSetup, ProtectionInfo } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { NumberField } from "@/components/ui/number-field";
import { Page } from "@/components/ui/page";
import { Select } from "@/components/ui/select";
import { ListSkeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Tooltip } from "@/components/ui/tooltip";
import {
  useBlockLists,
  useConfigureScheduledProtection,
  useCreateBlockList,
  useDeleteBlockList,
  useEnableProtection,
  useProtectionStatus,
  useRequestUnlockChallenge,
  useToggleBlockList,
  useUnlockProtection,
} from "@/lib/commands";
import { formatDuration } from "@/lib/duration";
import { m } from "@/paraglide/messages.js";

export function BlockLists() {
  const [name, setName] = useState("");
  const lists = useBlockLists();
  const protection = useProtectionStatus();
  const create = useCreateBlockList();

  const locks = protection.data ?? [];

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    create.mutate(name, { onSuccess: () => setName("") });
  }

  return (
    <Page>
      <PageHeader title={m.lists_title()} description={m.lists_description()} />

      <Card className="mb-6" padding="md" elevation="raised">
        <form onSubmit={onSubmit} className="flex gap-2">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={m.lists_new_placeholder()}
            aria-label={m.lists_new_label()}
          />
          <Button type="submit" icon={<Plus />} disabled={!name.trim() || create.isPending}>
            {create.isPending ? m.lists_creating() : m.lists_create()}
          </Button>
        </form>
        <InlineError error={create.error} />
      </Card>

      {lists.isPending ? (
        <ListSkeleton rows={3} />
      ) : (
        <QueryState
          isPending={false}
          error={lists.error}
          onRetry={() => lists.refetch()}
          isRetrying={lists.isFetching}
        >
          {lists.data?.length === 0 ? (
            <EmptyState
              icon={<ListChecks />}
              title={m.lists_empty_title()}
              description={m.lists_empty_description()}
            />
          ) : (
            <ul className="flex flex-col gap-2">
              {lists.data?.map((list) => (
                <ListRow
                  key={list.id}
                  list={list}
                  lock={locks.find((p) => p.block_list_id === list.id) ?? null}
                />
              ))}
            </ul>
          )}
        </QueryState>
      )}
    </Page>
  );
}

function effectiveLock(list: BlockList) {
  return list.protection && new Date(list.protection.expires_at).getTime() > Date.now()
    ? list.lock
    : (list.scheduled_protection?.lock ?? null);
}

function ListRow({ list, lock }: { list: BlockList; lock: ProtectionInfo | null }) {
  const toggle = useToggleBlockList();
  const remove = useDeleteBlockList();
  const [protecting, setProtecting] = useState(false);
  const [unlocking, setUnlocking] = useState(false);

  // The backend prioritizes a manual commitment when both protections overlap.
  // With no early-unlock method, the active protection must expire.
  const canUnlockEarly = lock !== null && effectiveLock(list) !== null;

  return (
    <li>
      <Card data-testid="block-list-row" elevation="raised" padding="none">
        <div className="flex items-center justify-between gap-4 px-4 py-3.5">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <p className="truncate font-medium text-foreground text-sm">{list.name}</p>
              {lock ? (
                <Badge tone="warning" icon={<Lock aria-hidden />} outlined>
                  {m.lists_badge_locked({ duration: formatDuration(lock.remaining_seconds) })}
                </Badge>
              ) : (
                <Badge tone={list.enabled ? "success" : "neutral"}>
                  {list.enabled ? m.lists_badge_enabled() : m.lists_badge_off()}
                </Badge>
              )}
              {list.schedule && <Badge tone="info">{m.lists_badge_scheduled()}</Badge>}
            </div>
            <p className="mt-1 text-faint-foreground text-xs">
              {m.count_sites({ count: list.websites.length })} ·{" "}
              {m.count_apps({ count: list.applications.length })} ·{" "}
              {m.count_exceptions({ count: list.exceptions.length })}
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-1">
            <Tooltip content={lock ? m.lists_toggle_locked() : m.lists_toggle_hint()}>
              <span>
                <Switch
                  checked={list.enabled}
                  onCheckedChange={(enabled) => toggle.mutate({ id: list.id, enabled })}
                  disabled={lock !== null && list.enabled}
                  aria-label={m.lists_enable({ name: list.name })}
                />
              </span>
            </Tooltip>

            <Tooltip
              content={
                lock === null
                  ? m.lists_protect_hint()
                  : canUnlockEarly
                    ? m.lists_unlock_hint()
                    : m.lists_unlock_none_hint()
              }
            >
              <span>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={
                    lock === null
                      ? m.lists_protect({ name: list.name })
                      : m.lists_unlock({ name: list.name })
                  }
                  aria-expanded={lock === null ? protecting : unlocking}
                  disabled={lock !== null && !canUnlockEarly}
                  onClick={() =>
                    lock === null ? setProtecting(!protecting) : setUnlocking(!unlocking)
                  }
                >
                  {lock === null ? <Lock /> : <Unlock />}
                </Button>
              </span>
            </Tooltip>

            <Tooltip content={lock ? m.lists_delete_locked() : m.lists_delete_hint()}>
              <span>
                <Button
                  variant="ghost"
                  tone="destructive"
                  size="icon"
                  aria-label={m.lists_delete({ name: list.name })}
                  disabled={lock !== null}
                  onClick={() => remove.mutate(list.id)}
                >
                  <Trash2 />
                </Button>
              </span>
            </Tooltip>
          </div>
        </div>

        {protecting && !lock && <ProtectForm list={list} onDone={() => setProtecting(false)} />}
        {unlocking && canUnlockEarly && (
          <UnlockForm list={list} onDone={() => setUnlocking(false)} />
        )}

        <InlineError error={toggle.error ?? remove.error} />
      </Card>
    </li>
  );
}

type LockKind = "none" | "password" | "random_text";

const LOCK_KIND_HINTS: Record<LockKind, () => string> = {
  none: m.lists_lock_kind_hint_none,
  password: m.lists_lock_kind_hint_password,
  random_text: m.lists_lock_kind_hint_random_text,
};

export function ProtectForm({
  list,
  onDone,
  scheduled = false,
}: {
  list: BlockList;
  onDone: () => void;
  scheduled?: boolean;
}) {
  const configure = useConfigureScheduledProtection();
  const [scheduleEnabled, setScheduleEnabled] = useState(!!list.scheduled_protection);
  const savedLock = scheduled ? list.scheduled_protection?.lock : null;
  const pending = configure.isPending;
  const protect = useEnableProtection();
  const id = useId();
  const passwordId = useId();
  const confirmId = useId();

  const [minutes, setMinutes] = useState(60);
  const [uninstall, setUninstall] = useState(true);
  const [serviceStop, setServiceStop] = useState(true);
  const [modification, setModification] = useState(true);

  const randomTextLengthId = useId();
  const [lockKind, setLockKind] = useState<LockKind>(
    savedLock && "Password" in savedLock
      ? "password"
      : savedLock && "RandomText" in savedLock
        ? "random_text"
        : "none",
  );
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [randomTextLength, setRandomTextLength] = useState(savedLock?.RandomText?.length ?? 16);

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
      <p className="mt-1 text-muted-foreground text-sm">
        {scheduled ? m.schedule_protection_description() : m.lists_lock_warning()}
      </p>
      {scheduled && (
        <Guard
          label={m.schedule_protection_label()}
          checked={scheduleEnabled}
          onChange={setScheduleEnabled}
        />
      )}
      {scheduled && (
        <p className="mt-2 text-muted-foreground text-xs">{m.schedule_protection_hint()}</p>
      )}

      {!scheduled && (
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
      )}

      {(!scheduled || scheduleEnabled) && (
        <>
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
                  max={64}
                />
              </div>
            )}
          </div>

          <p className="mt-2 text-faint-foreground text-xs">
            {passwordMismatch ? m.lists_lock_password_mismatch() : LOCK_KIND_HINTS[lockKind]()}
          </p>
        </>
      )}

      <div className="mt-4 flex items-center gap-2">
        <Button
          size="sm"
          tone="destructive"
          disabled={
            protect.isPending || pending || (!(scheduled && !scheduleEnabled) && !canSubmit)
          }
          onClick={() =>
            scheduled
              ? configure.mutate(
                  {
                    listId: list.id,
                    enabled: scheduleEnabled,
                    lock: scheduleEnabled ? buildLock() : null,
                  },
                  { onSuccess: onDone },
                )
              : protect.mutate(
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
          {scheduled
            ? pending
              ? m.schedule_saving()
              : m.schedule_protection_save()
            : protect.isPending
              ? m.lists_locking()
              : m.lists_lock_action({ duration: formatDuration(minutes * 60) })}
        </Button>
        {!scheduled && (
          <Button variant="ghost" size="sm" onClick={onDone}>
            {m.lists_cancel()}
          </Button>
        )}
      </div>

      <InlineError error={scheduled ? configure.error : protect.error} />
    </div>
  );
}

function UnlockForm({ list, onDone }: { list: BlockList; onDone: () => void }) {
  const unlock = useUnlockProtection();
  const requestChallenge = useRequestUnlockChallenge();
  const inputId = useId();
  const [response, setResponse] = useState("");

  const lock = effectiveLock(list);
  const isRandomText = lock !== null && "RandomText" in lock;

  // Random text needs a challenge to display before there is anything to
  // type. Asking twice is harmless: the backend returns the one already out.
  useEffect(() => {
    if (isRandomText) {
      requestChallenge.mutate(list.id);
    }
  }, [isRandomText, list.id, requestChallenge.mutate]);

  function submit() {
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
              {requestChallenge.data}
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
            if (e.key === "Enter" && response.length > 0) submit();
          }}
        />
      </div>

      <div className="mt-4 flex items-center gap-2">
        <Button
          size="sm"
          tone="destructive"
          disabled={
            unlock.isPending || response.length === 0 || (isRandomText && !requestChallenge.data)
          }
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
