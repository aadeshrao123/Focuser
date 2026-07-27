import { Plus, Trash2 } from "lucide-react";
import { type FormEvent, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  useBlockLists,
  useCreateBlockList,
  useDeleteBlockList,
  useToggleBlockList,
} from "@/lib/commands";
import { transportKind } from "@/lib/transport";
import { count } from "@/lib/utils";

export function BlockLists() {
  const [name, setName] = useState("");
  const lists = useBlockLists();
  const create = useCreateBlockList();
  const toggle = useToggleBlockList();
  const remove = useDeleteBlockList();

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    create.mutate(name, { onSuccess: () => setName("") });
  }

  return (
    <div className="p-8">
      <header className="mb-6 flex items-baseline justify-between">
        <div>
          <h1 className="font-semibold text-2xl text-foreground tracking-tight">Block Lists</h1>
          <p className="mt-1 text-muted-foreground text-sm">
            Group the sites and apps you want blocked together.
          </p>
        </div>
        <span
          className="rounded-md bg-surface px-2 py-1 text-faint-foreground text-xs"
          data-testid="transport-badge"
        >
          {transportKind}
        </span>
      </header>

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

      {create.error && (
        <p role="alert" className="mb-4 text-destructive text-sm">
          {create.error.message}
        </p>
      )}

      {lists.isPending && <p className="text-muted-foreground text-sm">Loading…</p>}

      {lists.error && (
        <div role="alert" className="rounded-md border border-destructive/40 bg-surface p-4">
          <p className="font-medium text-destructive text-sm">Couldn't load block lists</p>
          <p className="mt-1 text-muted-foreground text-sm">{lists.error.message}</p>
          <Button
            variant="outline"
            size="sm"
            className="mt-3"
            onClick={() => lists.refetch()}
            disabled={lists.isFetching}
          >
            Retry
          </Button>
        </div>
      )}

      {lists.data?.length === 0 && (
        <p className="text-muted-foreground text-sm">No block lists yet. Create one above.</p>
      )}

      <ul className="flex flex-col gap-2">
        {lists.data?.map((list) => (
          <li
            key={list.id}
            data-testid="block-list-row"
            className="flex items-center justify-between rounded-lg border border-border bg-surface px-4 py-3"
          >
            <div className="min-w-0">
              <p className="truncate font-medium text-foreground text-sm">{list.name}</p>
              <p className="text-faint-foreground text-xs">
                {count(list.websites.length, "site")} · {count(list.applications.length, "app")}
              </p>
            </div>

            <div className="flex shrink-0 items-center gap-2">
              <Button
                variant={list.enabled ? "soft" : "outline"}
                tone={list.enabled ? "success" : "default"}
                size="sm"
                onClick={() => toggle.mutate({ id: list.id, enabled: !list.enabled })}
                aria-pressed={list.enabled}
              >
                {list.enabled ? "Enabled" : "Disabled"}
              </Button>

              <Button
                variant="ghost"
                tone="destructive"
                size="icon"
                aria-label={`Delete ${list.name}`}
                onClick={() => remove.mutate(list.id)}
              >
                <Trash2 />
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
