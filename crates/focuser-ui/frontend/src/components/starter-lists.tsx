import { Download } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { InlineError, QueryState } from "@/components/ui/feedback";
import { useBulkImportWebsites } from "@/lib/commands";
import { type PremadeCategory, usePremadeLists } from "@/lib/premade";
import { count } from "@/lib/utils";

/** Curated categories, added to the current list in one click. */
export function StarterLists({ listId }: { listId: string }) {
  const categories = usePremadeLists();

  return (
    <>
      <p className="mb-4 text-muted-foreground text-sm">
        Ready-made sets of sites. Adding one appends to this block list; duplicates are skipped, so
        it is safe to add more than one.
      </p>

      <QueryState
        isPending={categories.isPending}
        error={categories.error}
        onRetry={() => categories.refetch()}
        isRetrying={categories.isFetching}
      >
        <div className="grid gap-3 sm:grid-cols-2">
          {categories.data?.map((category) => (
            <CategoryCard key={category.id} listId={listId} category={category} />
          ))}
        </div>
      </QueryState>
    </>
  );
}

function CategoryCard({ listId, category }: { listId: string; category: PremadeCategory }) {
  const importDomains = useBulkImportWebsites();
  const [added, setAdded] = useState<number | null>(null);

  // Domains and wildcards are different rule kinds, so they go in two passes.
  function add() {
    setAdded(null);
    importDomains.mutate(
      { listId, values: category.domains, kind: "domain" },
      {
        onSuccess: (first) => {
          const domains = first.kind === "count" ? first.data : 0;
          if (category.wildcards.length === 0) {
            setAdded(domains);
            return;
          }
          importDomains.mutate(
            { listId, values: category.wildcards, kind: "wildcard" },
            {
              onSuccess: (second) =>
                setAdded(domains + (second.kind === "count" ? second.data : 0)),
            },
          );
        },
      },
    );
  }

  const total = category.domains.length + category.wildcards.length;

  return (
    <Card>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="font-medium text-foreground text-sm">{category.name}</p>
          <p className="mt-0.5 text-muted-foreground text-xs">{category.description}</p>
          <p className="mt-1 text-faint-foreground text-xs">{count(total, "entry", "entries")}</p>
        </div>
        <Button
          variant="outline"
          size="sm"
          icon={<Download />}
          disabled={importDomains.isPending}
          onClick={add}
        >
          Add
        </Button>
      </div>

      {added !== null && (
        <p className="mt-2 text-success text-xs">
          Added {count(added, "new rule")}
          {added < total && ` · ${total - added} already there`}.
        </p>
      )}
      <InlineError error={importDomains.error} />
    </Card>
  );
}
