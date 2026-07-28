import { Download } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { InlineError } from "@/components/ui/feedback";
import { Select } from "@/components/ui/select";
import { useBulkImportWebsites } from "@/lib/commands";
import { usePremadeLists } from "@/lib/premade";
import { count } from "@/lib/utils";
import { m } from "@/paraglide/messages.js";

/**
 * Curated categories, added to the current list from one dropdown.
 *
 * Deliberately sitting next to the add-a-domain form rather than on a tab of
 * its own: importing a starter list is the same act as typing a domain, and
 * splitting them meant the fastest way to fill a list was the one people
 * never found.
 */
export function StarterLists({ listId }: { listId: string }) {
  const categories = usePremadeLists();
  const importer = useBulkImportWebsites();
  const [id, setId] = useState("");
  const [result, setResult] = useState<{ added: number; total: number } | null>(null);

  const all = categories.data ?? [];
  const chosen = all.find((c) => c.id === id) ?? all[0];

  const options = all.map((c) => ({
    value: c.id,
    label: `${c.name} · ${count(c.domains.length + c.wildcards.length, "site")}`,
  }));

  // Domains and wildcards are different rule kinds, so they go in two passes.
  function add() {
    if (!chosen) return;
    const total = chosen.domains.length + chosen.wildcards.length;
    setResult(null);

    importer.mutate(
      { listId, values: chosen.domains, kind: "domain" },
      {
        onSuccess: (first) => {
          const domains = first.kind === "count" ? first.data : 0;
          if (chosen.wildcards.length === 0) {
            setResult({ added: domains, total });
            return;
          }
          importer.mutate(
            { listId, values: chosen.wildcards, kind: "wildcard" },
            {
              onSuccess: (second) =>
                setResult({
                  added: domains + (second.kind === "count" ? second.data : 0),
                  total,
                }),
            },
          );
        },
      },
    );
  }

  if (categories.isPending) return null;

  return (
    <div className="mt-3 border-border border-t pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-muted-foreground text-sm">{m.starter_lists_label()}</span>
        <Select
          value={chosen?.id ?? ""}
          onValueChange={setId}
          options={options}
          size="sm"
          className="w-56"
          aria-label={m.starter_lists_picker_label()}
        />
        <Button
          variant="outline"
          size="sm"
          icon={<Download />}
          disabled={!chosen || importer.isPending}
          onClick={add}
        >
          {m.starter_lists_import()}
        </Button>
      </div>

      {chosen?.description && (
        <p className="mt-2 text-muted-foreground text-xs">{chosen.description}</p>
      )}

      {result && (
        <p className="mt-2 text-success text-xs">
          Added {count(result.added, "new site")}
          {result.added < result.total && ` · ${result.total - result.added} already there`}.
        </p>
      )}

      <InlineError error={categories.error ?? importer.error} />
    </div>
  );
}
