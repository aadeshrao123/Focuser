import { Lock, Unlock } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router-dom";
import type { BlockList, LockSetup } from "@/bindings";
import { effectiveLock, UnlockForm } from "@/components/focus-lock-forms";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { InlineError } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { NumberField } from "@/components/ui/number-field";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  useConfigureScheduledProtection,
  useProtectionStatus,
  useRelockScheduledProtection,
  useScheduledProtectionStatus,
} from "@/lib/commands";
import { m } from "@/paraglide/messages.js";

/** Both pages use this control, the same queries, and the same persisted setting. */
export function ScheduledProtectionControl({
  list,
  variant = "full",
}: {
  list: BlockList;
  variant?: "full" | "compact";
}) {
  const status = useScheduledProtectionStatus();
  const protection = useProtectionStatus();
  const configure = useConfigureScheduledProtection();
  const relock = useRelockScheduledProtection();
  const [unlocking, setUnlocking] = useState(false);
  const [kind, setKind] = useState<"random_text" | "password" | "none">("random_text");
  const [length, setLength] = useState(16);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [editingPassword, setEditingPassword] = useState(false);
  const state = status.data?.find((s) => s.block_list_id === list.id)?.state;
  const enabled = !!list.scheduled_protection;
  const protectedNow =
    protection.data?.some((p) => p.block_list_id === list.id && p.prevent_modification) ||
    state === "locked";
  const ready = kind !== "password" || (password.trim().length > 0 && password === confirm);
  const lock: LockSetup | null =
    kind === "none" ? null : kind === "password" ? { kind, password } : { kind, length };
  const savedLock = list.scheduled_protection?.lock;
  const savedKind = savedLock?.RandomText
    ? "random_text"
    : savedLock?.Password
      ? "password"
      : "none";
  const displayedKind = enabled ? (editingPassword ? "password" : savedKind) : kind;
  const displayedLength = enabled ? (savedLock?.RandomText?.length ?? 16) : length;
  const canEditEnabled = state === "unlocked_for_editing";
  const configurationDisabled =
    !state ||
    protection.isPending ||
    (enabled && !canEditEnabled) ||
    protectedNow ||
    configure.isPending;
  function saveLock(nextLock: LockSetup | null) {
    if (!canEditEnabled || configurationDisabled) return;
    configure.mutate(
      { listId: list.id, enabled: true, lock: nextLock },
      {
        onSuccess: () => {
          setEditingPassword(false);
          setPassword("");
          setConfirm("");
        },
      },
    );
  }
  function savePassword() {
    if (password.trim() && password === confirm) saveLock({ kind: "password", password });
  }
  // With protection off there is no persisted method. The compact toggle uses
  // the existing default; custom setup stays on Block Lists.
  const summaryKind = enabled ? savedKind : "random_text";
  const summary =
    summaryKind === "random_text"
      ? m.schedule_lock_random_summary({ count: enabled ? displayedLength : 16 })
      : summaryKind === "password"
        ? m.lists_lock_kind_password()
        : m.schedule_lock_none_summary();
  const labels = {
    off: m.schedule_lock_off,
    inactive: m.schedule_lock_inactive,
    locked: m.schedule_lock_locked,
    unlocked_for_editing: m.schedule_lock_editing,
  };

  return (
    <section className="border-border border-t p-4" aria-label={m.schedule_protection_label()}>
      <div className="flex flex-wrap items-center gap-3">
        <Switch
          checked={enabled}
          aria-label={m.schedule_protection_label()}
          disabled={
            !state ||
            protection.isPending ||
            protectedNow ||
            configure.isPending ||
            (!enabled && variant === "full" && !ready)
          }
          onCheckedChange={(next) =>
            configure.mutate({
              listId: list.id,
              enabled: next,
              lock: next
                ? variant === "compact"
                  ? { kind: "random_text", length: 16 }
                  : lock
                : null,
            })
          }
        />
        <span className="font-medium">{m.schedule_protection_label()}</span>
        {state && (
          <Badge
            size="md"
            outlined
            className="px-3 py-2 text-sm font-bold"
            tone={
              state === "locked"
                ? "warning"
                : state === "unlocked_for_editing"
                  ? "success"
                  : "neutral"
            }
            icon={
              state === "locked" ? (
                <Lock />
              ) : state === "unlocked_for_editing" ? (
                <Unlock />
              ) : undefined
            }
          >
            {labels[state]()}
          </Badge>
        )}
      </div>
      {variant === "compact" && (
        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
          <span className="text-muted-foreground">{summary}</span>
          <Link to="/block-lists" className="text-primary underline underline-offset-4">
            {m.schedule_lock_manage()}
          </Link>
        </div>
      )}
      {state === "inactive" && (
        <p className="mt-2 text-sm text-muted-foreground">{m.schedule_lock_next()}</p>
      )}
      {state === "locked" && (
        <div className="mt-3">
          <Button
            icon={<Unlock />}
            disabled={!effectiveLock(list)}
            onClick={() => setUnlocking(!unlocking)}
          >
            {m.schedule_lock_unlock()}
          </Button>
          {!effectiveLock(list) && (
            <p className="mt-2 text-sm text-muted-foreground">{m.lists_unlock_none_hint()}</p>
          )}
          {unlocking && effectiveLock(list) && (
            <UnlockForm list={list} onDone={() => setUnlocking(false)} />
          )}
        </div>
      )}
      {state === "unlocked_for_editing" && (
        <Button
          className="mt-3"
          icon={<Lock />}
          disabled={
            relock.isPending ||
            configure.isPending ||
            editingPassword ||
            password.length > 0 ||
            confirm.length > 0
          }
          onClick={() => relock.mutate(list.id)}
        >
          {m.schedule_lock_again()}
        </Button>
      )}
      {variant === "full" && (
        <div className="mt-3 space-y-3">
          {!protectedNow && (
            <p className="text-sm text-muted-foreground">{m.schedule_lock_setup()}</p>
          )}
          <Select
            value={displayedKind}
            onValueChange={(next) => {
              setKind(next);
              if (!enabled) return;
              setEditingPassword(next === "password");
              if (next !== "password")
                saveLock(next === "none" ? null : { kind: "random_text", length: displayedLength });
            }}
            disabled={configurationDisabled}
            aria-label={m.lists_lock_kind_label()}
            options={[
              { value: "random_text", label: m.lists_lock_kind_random_text() },
              { value: "password", label: m.lists_lock_kind_password() },
              { value: "none", label: m.lists_lock_kind_none() },
            ]}
          />
          {(!enabled || canEditEnabled) && !protectedNow && (
            <p className="text-sm text-muted-foreground">
              {displayedKind === "none"
                ? m.lists_lock_kind_hint_none()
                : displayedKind === "random_text"
                  ? m.lists_lock_kind_hint_random_text()
                  : confirm && password !== confirm
                    ? m.lists_lock_password_mismatch()
                    : m.lists_lock_kind_hint_password()}
            </p>
          )}
          {displayedKind === "random_text" && (
            <NumberField
              value={displayedLength}
              onCommit={(next) => {
                setLength(next);
                if (enabled) saveLock({ kind: "random_text", length: next });
              }}
              disabled={configurationDisabled}
              min={6}
              max={256}
              aria-label={m.lists_lock_random_text_length()}
            />
          )}
          {displayedKind === "password" && (!enabled || canEditEnabled) && (
            <div className="flex gap-2">
              <Input
                type="password"
                disabled={configurationDisabled}
                autoComplete="new-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                onBlur={savePassword}
                placeholder={m.lists_lock_password_placeholder()}
                aria-label={m.lists_lock_password_placeholder()}
              />
              <Input
                type="password"
                disabled={configurationDisabled}
                autoComplete="new-password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                onBlur={savePassword}
                placeholder={m.lists_lock_password_confirm_placeholder()}
                aria-label={m.lists_lock_password_confirm_placeholder()}
              />
            </div>
          )}
        </div>
      )}
      <InlineError error={configure.error ?? relock.error ?? status.error ?? protection.error} />
    </section>
  );
}
