import { ListChecks, Lock, Plus, Trash2, Unlock } from "lucide-react";
import { type FormEvent, useState } from "react";
import type { BlockList, ProtectionInfo } from "@/bindings";
import {
  effectiveLock,
  ProtectDialog,
  protectionActive,
  UnlockDialog,
} from "@/components/focus-lock-forms";
import { ListHoursOptions } from "@/components/list-hours-options";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { ErrorDialog } from "@/components/ui/dialog";
import { QueryState } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { Page } from "@/components/ui/page";
import { ListSkeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Tooltip } from "@/components/ui/tooltip";
import {
  useBlockLists,
  useCreateBlockList,
  useDeleteBlockList,
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
        <ErrorDialog error={create.error} onClose={() => create.reset()} />
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
                  {lock.remaining_seconds === null
                    ? m.lists_badge_locked_until_unlocked()
                    : m.lists_badge_locked({ duration: formatDuration(lock.remaining_seconds) })}
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

            {(!list.scheduled_protection || !lock || protectionActive(list.protection)) && (
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
                    aria-haspopup="dialog"
                    disabled={lock !== null && !canUnlockEarly}
                    onClick={() => (lock === null ? setProtecting(true) : setUnlocking(true))}
                  >
                    {lock === null ? <Lock /> : <Unlock />}
                  </Button>
                </span>
              </Tooltip>
            )}

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

        <ListHoursOptions list={list} page="lists" />

        {/* The card shows state. Setting a lock, ending one, and anything that
            goes wrong each get a pop-up, so the list never grows a form. */}
        <ProtectDialog
          list={list}
          open={protecting && !lock}
          onClose={() => setProtecting(false)}
        />
        <UnlockDialog
          list={list}
          open={unlocking && canUnlockEarly}
          onClose={() => setUnlocking(false)}
        />
        <ErrorDialog
          error={toggle.error ?? remove.error}
          onClose={() => {
            toggle.reset();
            remove.reset();
          }}
        />
      </Card>
    </li>
  );
}
