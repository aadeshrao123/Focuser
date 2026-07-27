import { Search, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { count } from "@/lib/utils";

export interface RuleRow {
  id: string;
  kind: string;
  value: string;
}

/** Below this a search box is more clutter than help. */
const SEARCH_THRESHOLD = 8;

/** Shared list for website rules, app rules and exceptions. */
export function RuleTable({
  rows,
  onRemove,
  noun = "rule",
}: {
  rows: RuleRow[];
  onRemove: (id: string) => void;
  /** Used in the search placeholder and the result count. */
  noun?: string;
}) {
  const [query, setQuery] = useState("");
  const searchable = rows.length > SEARCH_THRESHOLD;

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return rows;
    return rows.filter(
      (r) => r.value.toLowerCase().includes(needle) || r.kind.toLowerCase().includes(needle),
    );
  }, [rows, query]);

  return (
    <>
      {searchable && (
        <div className="relative mb-3 max-w-sm">
          <Search
            aria-hidden
            className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-faint-foreground"
          />
          <Input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`Search ${count(rows.length, noun)}…`}
            aria-label={`Search ${noun}s`}
            className="pl-8"
          />
        </div>
      )}

      {visible.length === 0 ? (
        <p className="text-muted-foreground text-sm">Nothing matches “{query}”.</p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {visible.map((row) => (
            <li
              key={row.id}
              data-testid="rule-row"
              className="flex items-center justify-between gap-3 rounded-md border border-border bg-surface px-3 py-2"
            >
              <div className="flex min-w-0 items-center gap-3">
                <span className="shrink-0 rounded bg-elevated px-1.5 py-0.5 text-faint-foreground text-xs">
                  {row.kind}
                </span>
                <span className="truncate text-foreground text-sm">{row.value || "—"}</span>
              </div>
              <Button
                variant="ghost"
                tone="destructive"
                size="icon"
                aria-label={`Remove ${row.value || row.kind}`}
                onClick={() => onRemove(row.id)}
              >
                <Trash2 />
              </Button>
            </li>
          ))}
        </ul>
      )}

      {searchable && query.trim() && visible.length > 0 && (
        <p className="mt-2 text-faint-foreground text-xs">
          Showing {visible.length} of {count(rows.length, noun)}.
        </p>
      )}
    </>
  );
}
