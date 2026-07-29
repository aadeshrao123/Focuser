import { ListChecks, Lock, Plus, Trash2 } from "lucide-react";
import { type FormEvent, useId, useState } from "react";
import type { BlockList, ProtectionInfo } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { NumberField } from "@/components/ui/number-field";
import { Page } from "@/components/ui/page";
import { ListSkeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Tooltip } from "@/components/ui/tooltip";
import {
  useBlockLists,
  useCreateBlockList,
  useDeleteBlockList,
  useEnableProtection,
  useProtectionStatus,
  useToggleBlockList,
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

function ListRow({ list, lock }: { list: BlockList; lock: ProtectionInfo | null }) {
  const toggle = useToggleBlockList();
  const remove = useDeleteBlockList();
  const [protecting, setProtecting] = useState(false);

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

            <Tooltip content={lock ? m.lists_protect_already() : m.lists_protect_hint()}>
              <span>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={m.lists_protect({ name: list.name })}
                  aria-expanded={protecting}
                  disabled={lock !== null}
                  onClick={() => setProtecting(!protecting)}
                >
                  <Lock />
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

        <InlineError error={toggle.error ?? remove.error} />
      </Card>
    </li>
  );
}

function ProtectForm({ list, onDone }: { list: BlockList; onDone: () => void }) {
  const protect = useEnableProtection();
  const id = useId();

  const [minutes, setMinutes] = useState(60);
  const [uninstall, setUninstall] = useState(true);
  const [serviceStop, setServiceStop] = useState(true);
  const [modification, setModification] = useState(true);

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

      <div className="mt-4 flex items-center gap-2">
        <Button
          size="sm"
          tone="destructive"
          disabled={protect.isPending}
          onClick={() =>
            protect.mutate(
              {
                listId: list.id,
                minutes,
                preventUninstall: uninstall,
                preventServiceStop: serviceStop,
                preventModification: modification,
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
