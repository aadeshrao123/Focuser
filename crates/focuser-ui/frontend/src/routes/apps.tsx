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
import { useAddAppRule, useAppIcons, useBlockLists, useRemoveAppRule } from "@/lib/commands";
import { APP_KINDS, type AppKind, appRule, describeApp } from "@/lib/match-types";
import { isTauri, pickApplication } from "@/lib/native";
import { m } from "@/paraglide/messages.js";

/**
 * Examples in the shape this machine actually reports.
 *
 * A rule is compared against a running process, and the three platforms name
 * those differently: `steam.exe` on Windows, `Steam` on macOS, `steam` on
 * Linux. Showing a Windows example to a Linux user invites a rule that can
 * never match.
 *
 * Read off the user agent rather than a Tauri plugin: this only picks example
 * text, and the preview harness gets it right for free.
 */
const EXAMPLES: Record<"windows" | "macos" | "linux", Record<AppKind, string>> = {
  windows: {
    ExecutableName: "discord.exe",
    ExecutablePath: "C:\\Program Files\\Steam\\steam.exe",
    WindowTitle: "Solitaire",
  },
  macos: {
    ExecutableName: "Discord",
    ExecutablePath: "/Applications/Steam.app",
    WindowTitle: "Solitaire",
  },
  linux: {
    ExecutableName: "discord",
    ExecutablePath: "/usr/bin/steam",
    WindowTitle: "Solitaire",
  },
};

function hostPlatform(): keyof typeof EXAMPLES {
  const agent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (agent.includes("Mac")) return "macos";
  if (agent.includes("Windows")) return "windows";
  return "linux";
}

const PLACEHOLDERS = EXAMPLES[hostPlatform()];

const APP_KIND_LABEL: Record<AppKind, () => string> = {
  ExecutableName: m.apps_kind_executable_name,
  ExecutablePath: m.apps_kind_executable_path,
  WindowTitle: m.apps_kind_window_title,
};

/** Built during render so the labels follow the current language. */
function appKindOptions() {
  return APP_KINDS.map((k) => ({ value: k.value, label: APP_KIND_LABEL[k.value]() }));
}

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
        title={m.apps_title()}
        description={m.apps_description()}
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
            title={m.websites_no_lists_title()}
            description={m.websites_no_lists_description()}
          />
        ) : (
          <>
            <Card className="mb-4" padding="md" elevation="raised">
              <form onSubmit={submit} className="flex flex-wrap gap-2">
                <Select
                  value={kind}
                  onValueChange={setKind}
                  options={appKindOptions()}
                  className="w-44"
                />
                <Input
                  value={value}
                  onChange={(e) => setValue(e.target.value)}
                  placeholder={PLACEHOLDERS[kind]}
                  aria-label={m.apps_value_label()}
                  className="max-w-sm flex-1"
                />
                <Button type="submit" icon={<Plus />} disabled={!value.trim() || add.isPending}>
                  {m.apps_add()}
                </Button>
                {isTauri() && (
                  <Button
                    type="button"
                    variant="outline"
                    icon={<FolderOpen />}
                    onClick={browse}
                    title={m.apps_browse_title()}
                  >
                    {m.apps_browse()}
                  </Button>
                )}
              </form>
              <InlineError error={add.error} />
            </Card>

            {list.applications.length === 0 ? (
              <EmptyState
                icon={<AppWindow />}
                title={m.apps_empty_title()}
                description={m.apps_empty_description({ example: PLACEHOLDERS.ExecutableName })}
              />
            ) : (
              <AppRules
                listId={list.id}
                rules={list.applications}
                onRemove={(ruleId) => remove.mutate({ listId: list.id, ruleId })}
              />
            )}
            <InlineError error={remove.error} />
          </>
        )}
      </QueryState>
    </Page>
  );
}

/**
 * The rule list, plus each program's real icon where the machine has one.
 *
 * Split out so the icon query is keyed by exactly the rules on screen, and so
 * a slow disk read cannot hold up the rest of the page.
 */
function AppRules({
  listId,
  rules,
  onRemove,
}: {
  listId: string;
  rules: { id: string; match_type: Parameters<typeof describeApp>[0] }[];
  onRemove: (ruleId: string) => void;
}) {
  const rows = rules.map((r) => ({ id: r.id, ...describeApp(r.match_type) }));
  const icons = useAppIcons(rows.map((r) => r.value).filter(Boolean));

  return (
    <RuleTable
      key={listId}
      rows={rows}
      onRemove={onRemove}
      noun={m.noun_application()}
      icons={icons.data}
    />
  );
}
