import { Ban, Globe, Plus, ShieldCheck, TriangleAlert } from "lucide-react";
import { type FormEvent, useState } from "react";
import { ListPicker, resolveSelected } from "@/components/list-picker";
import { RuleTable } from "@/components/rule-table";
import { StarterLists } from "@/components/starter-lists";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, PageHeader } from "@/components/ui/card";
import { ConfirmButton } from "@/components/ui/confirm-button";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { Page } from "@/components/ui/page";
import { Select } from "@/components/ui/select";
import { Tabs } from "@/components/ui/tabs";
import {
  useAddException,
  useAddWebsiteRule,
  useBlockingHealth,
  useBlockLists,
  useBulkImportWebsites,
  useRemoveException,
  useRemoveWebsiteRule,
} from "@/lib/commands";
import {
  describeException,
  describeWebsite,
  EXCEPTION_KINDS,
  type ExceptionKind,
  exceptionRule,
  IMPORT_KINDS,
  type ImportKind,
  WEBSITE_KINDS,
  type WebsiteKind,
  websiteRule,
} from "@/lib/match-types";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages.js";

type Tab = "blocked" | "exceptions" | "import";

/**
 * Called during render, never at module scope.
 *
 * A message function reads the current locale when it runs. Assigning one to a
 * module constant would freeze whatever locale was active at import time, and
 * switching language would leave it behind.
 */
const KIND_LABEL: Record<WebsiteKind, () => string> = {
  Domain: m.websites_kind_domain,
  Keyword: m.websites_kind_keyword,
  Wildcard: m.websites_kind_wildcard,
  UrlPath: m.websites_kind_url_path,
  EntireInternet: m.websites_kind_entire_internet,
};

/** Options for the rule-kind picker, labelled in the current language. */
function websiteKindOptions() {
  return WEBSITE_KINDS.map((k) => ({ value: k.value, label: KIND_LABEL[k.value]() }));
}

function importKindOptions() {
  return IMPORT_KINDS.map((k) => ({ value: k.value, label: KIND_LABEL[k.value]() }));
}

function exceptionKindOptions() {
  return EXCEPTION_KINDS.map((k) => ({
    value: k.value,
    label: k.value === "Domain" ? m.websites_kind_domain() : m.websites_kind_wildcard(),
  }));
}

function kindHelp(kind: WebsiteKind): { placeholder: string; hint: string } {
  switch (kind) {
    case "Domain":
      return { placeholder: "reddit.com", hint: m.websites_hint_domain() };
    case "Keyword":
      return { placeholder: "casino", hint: m.websites_hint_keyword() };
    case "Wildcard":
      return { placeholder: "*.reddit.com", hint: m.websites_hint_wildcard() };
    case "UrlPath":
      return { placeholder: "/r/gaming", hint: m.websites_hint_url_path() };
    case "EntireInternet":
      return { placeholder: "", hint: m.websites_hint_entire_internet() };
  }
}

/** `*`, `*.*`, `**`: a pattern with nothing in it but wildcards. */
const CATCH_ALL = /^[*.\s]+$/;

export function Websites() {
  const lists = useBlockLists();
  const health = useBlockingHealth();
  const [rawSelected, setSelected] = useState("");
  const [tab, setTab] = useState<Tab>("blocked");

  const all = lists.data ?? [];
  const selected = resolveSelected(all, rawSelected);
  const list = all.find((l) => l.id === selected);

  // A hosts file cannot express a keyword, wildcard or path, so without the
  // extension those rules do nothing at all. Saying so beats a silent miss.
  const unenforced = health.data?.extension_only_rules && !health.data.extension_connected;

  return (
    <Page>
      <PageHeader
        title={m.websites_title()}
        description={m.websites_description()}
        actions={<ListPicker lists={all} value={selected} onChange={setSelected} />}
      />

      {unenforced && <ExtensionNeeded />}

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
            <Tabs
              className="mb-5"
              value={tab}
              onChange={setTab}
              items={[
                { id: "blocked", label: m.websites_tab_blocked(), count: list.websites.length },
                {
                  id: "exceptions",
                  label: m.websites_tab_exceptions(),
                  count: list.exceptions.length,
                },
                { id: "import", label: m.websites_tab_import() },
              ]}
            />

            {tab === "blocked" && <BlockedTab listId={list.id} rules={list.websites} />}
            {tab === "exceptions" && (
              <ExceptionsTab listId={list.id} exceptions={list.exceptions} />
            )}
            {tab === "import" && <ImportTab listId={list.id} />}
          </>
        )}
      </QueryState>
    </Page>
  );
}

function ExtensionNeeded() {
  return (
    <Card className="mb-6 border-warning/40" padding="md">
      <p className="flex items-center gap-2 font-medium text-sm text-warning">
        <TriangleAlert aria-hidden className="size-4" />
        {m.websites_extension_needed_title()}
      </p>
      <p className="mt-1 text-muted-foreground text-sm">{m.websites_extension_needed_body()}</p>
    </Card>
  );
}

function BlockedTab({
  listId,
  rules,
}: {
  listId: string;
  rules: { id: string; match_type: Parameters<typeof describeWebsite>[0] }[];
}) {
  const [value, setValue] = useState("");
  const [kind, setKind] = useState<WebsiteKind>("Domain");
  const add = useAddWebsiteRule();
  const remove = useRemoveWebsiteRule();

  const wholeInternet = kind === "EntireInternet";
  const trimmed = value.trim();
  const catchAll = wholeInternet || (kind === "Wildcard" && !!trimmed && CATCH_ALL.test(trimmed));

  function addRule() {
    if (!wholeInternet && !trimmed) return;
    add.mutate({ listId, rule: websiteRule(kind, trimmed) }, { onSuccess: () => setValue("") });
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    // Blocking everything takes a deliberate second click, not a stray Enter.
    if (catchAll) return;
    addRule();
  }

  const help = kindHelp(kind);

  return (
    <>
      <Card className="mb-4" padding="md" elevation="raised">
        <form onSubmit={submit} className="flex flex-wrap gap-2">
          <Select
            value={kind}
            onValueChange={setKind}
            options={websiteKindOptions()}
            className="w-40"
          />
          {!wholeInternet && (
            <Input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder={help.placeholder}
              aria-label={m.websites_value_label()}
              className="max-w-sm flex-1"
            />
          )}
          {catchAll ? (
            <ConfirmButton
              icon={<Ban />}
              confirmLabel={m.websites_confirm_block_everything()}
              onConfirm={addRule}
              disabled={add.isPending}
            >
              {m.websites_add()}
            </ConfirmButton>
          ) : (
            <Button type="submit" icon={<Plus />} disabled={!trimmed || add.isPending}>
              {m.websites_add()}
            </Button>
          )}
        </form>
        <p className={cn("mt-2 text-xs", catchAll ? "text-warning" : "text-muted-foreground")}>
          {catchAll && !wholeInternet ? m.websites_hint_catch_all() : help.hint}
        </p>
        <InlineError error={add.error} />
        <StarterLists listId={listId} />
      </Card>

      {rules.length === 0 ? (
        <EmptyState
          icon={<Globe />}
          title={m.websites_empty_title()}
          description={m.websites_empty_description()}
        />
      ) : (
        <RuleTable
          rows={rules.map((r) => ({ id: r.id, ...describeWebsite(r.match_type) }))}
          onRemove={(ruleId) => remove.mutate({ listId, ruleId })}
          noun="rule"
        />
      )}
      <InlineError error={remove.error} />
    </>
  );
}

function ExceptionsTab({
  listId,
  exceptions,
}: {
  listId: string;
  exceptions: { id: string; exception_type: Parameters<typeof describeException>[0] }[];
}) {
  const [value, setValue] = useState("");
  const [kind, setKind] = useState<ExceptionKind>("Domain");
  const add = useAddException();
  const remove = useRemoveException();

  function submit(e: FormEvent) {
    e.preventDefault();
    const trimmed = value.trim();
    if (!trimmed) return;
    add.mutate(
      { listId, exception: exceptionRule(kind, trimmed) },
      { onSuccess: () => setValue("") },
    );
  }

  return (
    <>
      <p className="mb-4 text-muted-foreground text-sm">{m.exceptions_intro()}</p>

      <Card className="mb-4" padding="md" elevation="raised">
        <form onSubmit={submit} className="flex flex-wrap gap-2">
          <Select
            value={kind}
            onValueChange={setKind}
            options={exceptionKindOptions()}
            className="w-40"
          />
          <Input
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="docs.example.com"
            aria-label={m.exceptions_value_label()}
            className="max-w-sm flex-1"
          />
          <Button type="submit" icon={<Plus />} disabled={!value.trim() || add.isPending}>
            {m.exceptions_allow()}
          </Button>
        </form>
        <InlineError error={add.error} />
      </Card>

      {exceptions.length === 0 ? (
        <EmptyState
          icon={<ShieldCheck />}
          title={m.exceptions_empty_title()}
          description={m.exceptions_empty_description()}
        />
      ) : (
        <RuleTable
          rows={exceptions.map((e) => ({ id: e.id, ...describeException(e.exception_type) }))}
          onRemove={(exceptionId) => remove.mutate({ listId, exceptionId })}
          noun="exception"
        />
      )}
      <InlineError error={remove.error} />
    </>
  );
}

function ImportTab({ listId }: { listId: string }) {
  const [text, setText] = useState("");
  const [kind, setKind] = useState<ImportKind>("Domain");
  const importer = useBulkImportWebsites();

  const kindMap = {
    Domain: "domain",
    Keyword: "keyword",
    Wildcard: "wildcard",
    UrlPath: "url_path",
  } as const;

  function submit(e: FormEvent) {
    e.preventDefault();
    const values = text.split("\n");
    if (values.length === 0) return;
    importer.mutate({ listId, values, kind: kindMap[kind] }, { onSuccess: () => setText("") });
  }

  const added = importer.data?.kind === "count" ? importer.data.data : null;

  return (
    <form onSubmit={submit}>
      <p className="mb-3 text-muted-foreground text-sm">{m.import_intro()}</p>

      <div className="mb-3 flex gap-2">
        <Select value={kind} onValueChange={setKind} options={importKindOptions()} />
        <Button type="submit" disabled={!text.trim() || importer.isPending}>
          {importer.isPending ? m.import_pending() : m.import_action()}
        </Button>
      </div>

      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        aria-label={m.import_values_label()}
        rows={12}
        spellCheck={false}
        placeholder={"reddit.com\ntwitter.com\n# social\nfacebook.com"}
        className="w-full rounded-md border border-border bg-surface p-3 font-mono text-foreground text-sm placeholder:text-faint-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      />

      {added !== null && (
        <p className="mt-2 text-success text-sm">{m.import_added({ count: added })}</p>
      )}
      <InlineError error={importer.error} />
    </form>
  );
}
