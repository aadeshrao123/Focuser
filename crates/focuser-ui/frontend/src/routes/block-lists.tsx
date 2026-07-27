import { Lock, Plus, Trash2 } from "lucide-react";
import { type FormEvent, useId, useState } from "react";
import type { BlockList, ProtectionInfo } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { NumberField } from "@/components/ui/number-field";
import { Switch } from "@/components/ui/switch";
import {
  useBlockLists,
  useCreateBlockList,
  useDeleteBlockList,
  useEnableProtection,
  useProtectionStatus,
  useToggleBlockList,
} from "@/lib/commands";
import { formatDuration } from "@/lib/duration";
import { transportKind } from "@/lib/transport";
import { count } from "@/lib/utils";

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
    <div className="p-8">
      <PageHeader
        title="Block Lists"
        description="Group the sites and apps you want blocked together."
        actions={
          <span
            className="rounded-md bg-surface px-2 py-1 text-faint-foreground text-xs"
            data-testid="transport-badge"
          >
            {transportKind}
          </span>
        }
      />

      <form onSubmit={onSubmit} className="mb-6 flex gap-2">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="New block list name…"
          aria-label="New block list name"
          className="max-w-sm"
        />
        <Button type="submit" icon={<Plus />} disabled={!name.trim() || create.isPending}>
          {create.isPending ? "Creating…" : "Create"}
        </Button>
      </form>
      <InlineError error={create.error} />

      <QueryState
        isPending={lists.isPending}
        error={lists.error}
        onRetry={() => lists.refetch()}
        isRetrying={lists.isFetching}
      >
        {lists.data?.length === 0 ? (
          <EmptyState
            title="No block lists yet"
            description="Create one above, then add sites and apps to it."
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
    </div>
  );
}

function ListRow({ list, lock }: { list: BlockList; lock: ProtectionInfo | null }) {
  const toggle = useToggleBlockList();
  const remove = useDeleteBlockList();
  const [protecting, setProtecting] = useState(false);

  return (
    <li>
      <Card data-testid="block-list-row">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <p className="truncate font-medium text-foreground text-sm">{list.name}</p>
            <p className="text-faint-foreground text-xs">
              {count(list.websites.length, "site")} · {count(list.applications.length, "app")}
              {lock && ` · locked for ${formatDuration(lock.remaining_seconds)}`}
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant={list.enabled ? "soft" : "outline"}
              tone={list.enabled ? "success" : "default"}
              size="sm"
              onClick={() => toggle.mutate({ id: list.id, enabled: !list.enabled })}
              aria-pressed={list.enabled}
              // A lock exists to stop you turning blocking off mid-commitment.
              disabled={lock !== null && list.enabled}
            >
              {list.enabled ? "Enabled" : "Disabled"}
            </Button>

            <Button
              variant="ghost"
              size="icon"
              aria-label={`Protect ${list.name}`}
              aria-expanded={protecting}
              disabled={lock !== null}
              onClick={() => setProtecting(!protecting)}
            >
              <Lock />
            </Button>

            <Button
              variant="ghost"
              tone="destructive"
              size="icon"
              aria-label={`Delete ${list.name}`}
              disabled={lock !== null}
              onClick={() => remove.mutate(list.id)}
            >
              <Trash2 />
            </Button>
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
    <div className="mt-4 border-border border-t pt-4">
      <p className="text-muted-foreground text-sm">
        Locks this list on for a set time. There is no way to cancel it early — that is the point.
      </p>

      <div className="mt-3 flex flex-wrap items-center gap-x-6 gap-y-3">
        <div className="flex items-center gap-2">
          <label htmlFor={id} className="text-muted-foreground text-sm">
            Lock for
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

        <Guard label="Block uninstalling" checked={uninstall} onChange={setUninstall} />
        <Guard label="Block stopping the service" checked={serviceStop} onChange={setServiceStop} />
        <Guard label="Block editing the list" checked={modification} onChange={setModification} />
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
          {protect.isPending ? "Locking…" : `Lock for ${formatDuration(minutes * 60)}`}
        </Button>
        <Button variant="ghost" size="sm" onClick={onDone}>
          Cancel
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
