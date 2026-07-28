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

type Tab = "blocked" | "exceptions" | "import";

const KIND_HELP: Record<WebsiteKind, { placeholder: string; hint: string }> = {
  Domain: { placeholder: "reddit.com", hint: "Covers www. and every subdomain." },
  Keyword: {
    placeholder: "casino",
    hint: "Blocks any URL containing this word. Extension only.",
  },
  Wildcard: {
    placeholder: "*.reddit.com",
    hint: "* matches any run of characters, and *.reddit.com covers reddit.com too. Extension only.",
  },
  UrlPath: {
    placeholder: "/r/gaming",
    hint: "Blocks URLs containing this path. Extension only.",
  },
  EntireInternet: {
    placeholder: "",
    hint: "Blocks every site except your exceptions, so add those first. Extension only.",
  },
};

/** `*`, `*.*`, `**`: a pattern with nothing in it but wildcards. */
const CATCH_ALL = /^[*.\s]+$/;

const CATCH_ALL_WARNING =
  "This matches every site, including local pages and your own machine. Pick Entire internet instead if that is what you want, and add exceptions first.";

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
        title="Websites"
        description="Domains, keywords and URL patterns to block."
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
            title="No block lists yet"
            description="Create one on the Block Lists page first."
          />
        ) : (
          <>
            <Tabs
              className="mb-5"
              value={tab}
              onChange={setTab}
              items={[
                { id: "blocked", label: "Blocked", count: list.websites.length },
                { id: "exceptions", label: "Exceptions", count: list.exceptions.length },
                { id: "import", label: "Bulk import" },
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
        Keyword, wildcard and URL path rules need the browser extension
      </p>
      <p className="mt-1 text-muted-foreground text-sm">
        Focuser blocks plain domains through the hosts file, but that file has no way to express a
        pattern. Those rules are being ignored right now. Install the extension from Settings and
        they start working straight away. Domain rules are unaffected.
      </p>
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

  return (
    <>
      <Card className="mb-4" padding="md" elevation="raised">
        <form onSubmit={submit} className="flex flex-wrap gap-2">
          <Select value={kind} onValueChange={setKind} options={WEBSITE_KINDS} className="w-40" />
          {!wholeInternet && (
            <Input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder={KIND_HELP[kind].placeholder}
              aria-label="Value to block"
              className="max-w-sm flex-1"
            />
          )}
          {catchAll ? (
            <ConfirmButton
              icon={<Ban />}
              confirmLabel="Block every site?"
              onConfirm={addRule}
              disabled={add.isPending}
            >
              Add
            </ConfirmButton>
          ) : (
            <Button type="submit" icon={<Plus />} disabled={!trimmed || add.isPending}>
              Add
            </Button>
          )}
        </form>
        <p className={cn("mt-2 text-xs", catchAll ? "text-warning" : "text-muted-foreground")}>
          {wholeInternet
            ? KIND_HELP[kind].hint
            : catchAll
              ? CATCH_ALL_WARNING
              : KIND_HELP[kind].hint}
        </p>
        <InlineError error={add.error} />
        <StarterLists listId={listId} />
      </Card>

      {rules.length === 0 ? (
        <EmptyState
          icon={<Globe />}
          title="Nothing blocked yet"
          description="Add a domain above, or import one of the curated starter lists."
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
      <p className="mb-4 text-muted-foreground text-sm">
        Exceptions stay reachable even when a rule above would block them.
      </p>

      <Card className="mb-4" padding="md" elevation="raised">
        <form onSubmit={submit} className="flex flex-wrap gap-2">
          <Select value={kind} onValueChange={setKind} options={EXCEPTION_KINDS} className="w-40" />
          <Input
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="docs.example.com"
            aria-label="Value to allow"
            className="max-w-sm flex-1"
          />
          <Button type="submit" icon={<Plus />} disabled={!value.trim() || add.isPending}>
            Allow
          </Button>
        </form>
        <InlineError error={add.error} />
      </Card>

      {exceptions.length === 0 ? (
        <EmptyState
          icon={<ShieldCheck />}
          title="No exceptions"
          description="An exception stays reachable even when a rule above would block it."
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
      <p className="mb-3 text-muted-foreground text-sm">
        One value per line. Blank lines, duplicates and <code>#</code> comments are skipped.
      </p>

      <div className="mb-3 flex gap-2">
        <Select value={kind} onValueChange={setKind} options={IMPORT_KINDS} />
        <Button type="submit" disabled={!text.trim() || importer.isPending}>
          {importer.isPending ? "Importing…" : "Import"}
        </Button>
      </div>

      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        aria-label="Values to import"
        rows={12}
        spellCheck={false}
        placeholder={"reddit.com\ntwitter.com\n# social\nfacebook.com"}
        className="w-full rounded-md border border-border bg-surface p-3 font-mono text-foreground text-sm placeholder:text-faint-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      />

      {added !== null && (
        <p className="mt-2 text-success text-sm">
          Imported {added} {added === 1 ? "rule" : "rules"}.
        </p>
      )}
      <InlineError error={importer.error} />
    </form>
  );
}
