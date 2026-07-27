import { Plus } from "lucide-react";
import { type FormEvent, useState } from "react";
import { ListPicker, resolveSelected } from "@/components/list-picker";
import { RuleTable } from "@/components/rule-table";
import { StarterLists } from "@/components/starter-lists";
import { Button } from "@/components/ui/button";
import { EmptyState, PageHeader } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  useAddException,
  useAddWebsiteRule,
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
  WEBSITE_KINDS,
  type WebsiteKind,
  websiteRule,
} from "@/lib/match-types";
import { cn } from "@/lib/utils";

type Tab = "blocked" | "exceptions" | "starter" | "import";

export function Websites() {
  const lists = useBlockLists();
  const [rawSelected, setSelected] = useState("");
  const [tab, setTab] = useState<Tab>("blocked");

  const all = lists.data ?? [];
  const selected = resolveSelected(all, rawSelected);
  const list = all.find((l) => l.id === selected);

  return (
    <div className="p-8">
      <PageHeader
        title="Websites"
        description="Domains, keywords and URL patterns to block."
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
            <Tabs value={tab} onChange={setTab} />

            {tab === "blocked" && <BlockedTab listId={list.id} rules={list.websites} />}
            {tab === "exceptions" && (
              <ExceptionsTab listId={list.id} exceptions={list.exceptions} />
            )}
            {tab === "starter" && <StarterLists listId={list.id} />}
            {tab === "import" && <ImportTab listId={list.id} />}
          </>
        )}
      </QueryState>
    </div>
  );
}

const TABS: { id: Tab; label: string }[] = [
  { id: "blocked", label: "Blocked" },
  { id: "exceptions", label: "Exceptions" },
  { id: "starter", label: "Starter lists" },
  { id: "import", label: "Bulk import" },
];

function Tabs({ value, onChange }: { value: Tab; onChange: (t: Tab) => void }) {
  return (
    <div role="tablist" className="mb-5 flex gap-1 border-border border-b">
      {TABS.map((t) => (
        <button
          key={t.id}
          type="button"
          role="tab"
          aria-selected={value === t.id}
          onClick={() => onChange(t.id)}
          className={cn(
            "-mb-px border-b-2 px-3 py-2 text-sm transition-colors",
            value === t.id
              ? "border-primary text-foreground"
              : "border-transparent text-muted-foreground hover:text-foreground",
          )}
        >
          {t.label}
        </button>
      ))}
    </div>
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

  function submit(e: FormEvent) {
    e.preventDefault();
    const trimmed = value.trim();
    if (!trimmed) return;
    add.mutate({ listId, rule: websiteRule(kind, trimmed) }, { onSuccess: () => setValue("") });
  }

  return (
    <>
      <form onSubmit={submit} className="mb-4 flex flex-wrap gap-2">
        <Select value={kind} onValueChange={setKind} options={WEBSITE_KINDS} />
        <Input
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder="reddit.com"
          aria-label="Value to block"
          className="max-w-sm"
        />
        <Button type="submit" icon={<Plus />} disabled={!value.trim() || add.isPending}>
          Add
        </Button>
      </form>
      <InlineError error={add.error} />

      {rules.length === 0 ? (
        <EmptyState title="Nothing blocked yet" description="Add a domain above to get started." />
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

      <form onSubmit={submit} className="mb-4 flex flex-wrap gap-2">
        <Select value={kind} onValueChange={setKind} options={EXCEPTION_KINDS} />
        <Input
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder="docs.example.com"
          aria-label="Value to allow"
          className="max-w-sm"
        />
        <Button type="submit" icon={<Plus />} disabled={!value.trim() || add.isPending}>
          Allow
        </Button>
      </form>
      <InlineError error={add.error} />

      {exceptions.length === 0 ? (
        <EmptyState title="No exceptions" />
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
  const [kind, setKind] = useState<WebsiteKind>("Domain");
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
        <Select value={kind} onValueChange={setKind} options={WEBSITE_KINDS} />
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
