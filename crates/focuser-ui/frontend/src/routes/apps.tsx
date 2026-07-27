import { AppWindow, FolderOpen, Plus } from "lucide-react";
import { type FormEvent, useState } from "react";
import { ListPicker, resolveSelected } from "@/components/list-picker";
import { RuleTable } from "@/components/rule-table";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { Page } from "@/components/ui/page";
import { Select } from "@/components/ui/select";
import { useAddAppRule, useBlockLists, useRemoveAppRule } from "@/lib/commands";
import { APP_KINDS, type AppKind, appRule, describeApp } from "@/lib/match-types";
import { isTauri, pickApplication } from "@/lib/native";

const PLACEHOLDERS: Record<AppKind, string> = {
  ExecutableName: "discord.exe",
  ExecutablePath: "C:\\Program Files\\Steam\\steam.exe",
  WindowTitle: "Solitaire",
};

export function Apps() {
  const lists = useBlockLists();
  const [rawSelected, setSelected] = useState("");
  const [value, setValue] = useState("");
  const [kind, setKind] = useState<AppKind>("ExecutableName");

  const add = useAddAppRule();
  const remove = useRemoveAppRule();

  const all = lists.data ?? [];
  const selected = resolveSelected(all, rawSelected);
  const list = all.find((l) => l.id === selected);

  /** Picking from disk gives an executable name, which is the default rule kind. */
  async function browse() {
    const name = await pickApplication();
    if (!name) return;
    setKind("ExecutableName");
    setValue(name);
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    const trimmed = value.trim();
    if (!list || !trimmed) return;
    add.mutate(
      { listId: list.id, rule: appRule(kind, trimmed) },
      { onSuccess: () => setValue("") },
    );
  }

  return (
    <Page>
      <PageHeader
        title="Applications"
        description="Programs to close while blocking is active."
        actions={<ListPicker lists={all} value={selected} onChange={setSelected} />}
      />

      <QueryState
        isPending={lists.isPending}
        error={lists.error}
        onRetry={() => lists.refetch()}
        isRetrying={lists.isFetching}
      >
        {!list ? (
          <EmptyState
            title="No block lists yet"
            description="Create one on the Block Lists page first."
          />
        ) : (
          <>
            <Card className="mb-4" padding="md" elevation="raised">
              <form onSubmit={submit} className="flex flex-wrap gap-2">
                <Select value={kind} onValueChange={setKind} options={APP_KINDS} className="w-44" />
                <Input
                  value={value}
                  onChange={(e) => setValue(e.target.value)}
                  placeholder={PLACEHOLDERS[kind]}
                  aria-label="Application to block"
                  className="max-w-sm flex-1"
                />
                <Button type="submit" icon={<Plus />} disabled={!value.trim() || add.isPending}>
                  Add
                </Button>
                {isTauri() && (
                  <Button
                    type="button"
                    variant="outline"
                    icon={<FolderOpen />}
                    onClick={browse}
                    title="Pick an application from disk"
                  >
                    Browse…
                  </Button>
                )}
              </form>
              <InlineError error={add.error} />
            </Card>

            {list.applications.length === 0 ? (
              <EmptyState
                icon={<AppWindow />}
                title="No applications blocked"
                description="Add an executable name like discord.exe, or browse for one on disk."
              />
            ) : (
              <RuleTable
                rows={list.applications.map((r) => ({ id: r.id, ...describeApp(r.match_type) }))}
                onRemove={(ruleId) => remove.mutate({ listId: list.id, ruleId })}
                noun="application"
              />
            )}
            <InlineError error={remove.error} />
          </>
        )}
      </QueryState>
    </Page>
  );
}
