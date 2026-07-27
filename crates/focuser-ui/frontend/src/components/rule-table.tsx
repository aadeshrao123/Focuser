import {
  Asterisk,
  Ban,
  Captions,
  FileText,
  FolderOpen,
  Globe,
  Hash,
  Link2,
  MonitorSmartphone,
  Package,
  Search,
  Trash2,
} from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { Badge, type BadgeProps } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { TargetIcon } from "@/components/ui/target-icon";
import { count } from "@/lib/utils";

export interface RuleRow {
  id: string;
  kind: string;
  value: string;
}

/** Below this a search box is more clutter than help. */
const SEARCH_THRESHOLD = 8;

/**
 * How each rule kind is dressed.
 *
 * Keyed by the label `describeWebsite`/`describeApp`/`describeException`
 * produce. A kind with no entry still renders — it just gets the neutral
 * treatment rather than crashing on a variant added later in Rust.
 */
const KINDS: Record<string, { tone: BadgeProps["tone"]; icon: ReactNode; named: boolean }> = {
  Domain: { tone: "primary", icon: <Globe />, named: true },
  Keyword: { tone: "info", icon: <Hash />, named: false },
  Wildcard: { tone: "warning", icon: <Asterisk />, named: false },
  "URL path": { tone: "success", icon: <Link2 />, named: false },
  "Entire internet": { tone: "destructive", icon: <Ban />, named: false },
  Executable: { tone: "primary", icon: <MonitorSmartphone />, named: true },
  // A full path's first character is the drive letter or a slash, which says
  // nothing about what the rule is — the folder glyph carries more.
  Path: { tone: "info", icon: <FolderOpen />, named: false },
  "Window title": { tone: "warning", icon: <Captions />, named: false },
  "Bundle ID": { tone: "info", icon: <Package />, named: true },
  "Local files": { tone: "success", icon: <FileText />, named: false },
};

const styleFor = (kind: string) =>
  KINDS[kind] ?? { tone: "neutral" as const, icon: null, named: true };

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
  const [kindFilter, setKindFilter] = useState<string | null>(null);
  const searchable = rows.length > SEARCH_THRESHOLD;

  // Only worth offering when the list actually mixes kinds.
  const kinds = useMemo(() => {
    const tally = new Map<string, number>();
    for (const r of rows) tally.set(r.kind, (tally.get(r.kind) ?? 0) + 1);
    return [...tally].sort((a, b) => b[1] - a[1]);
  }, [rows]);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return rows.filter((r) => {
      if (kindFilter && r.kind !== kindFilter) return false;
      if (!needle) return true;
      return r.value.toLowerCase().includes(needle) || r.kind.toLowerCase().includes(needle);
    });
  }, [rows, query, kindFilter]);

  const filtered = visible.length !== rows.length;

  return (
    <>
      {(searchable || kinds.length > 1) && (
        <div className="mb-3 flex flex-wrap items-center gap-2">
          {searchable && (
            <div className="relative min-w-56 flex-1 sm:max-w-xs">
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

          {kinds.length > 1 && (
            <div className="flex flex-wrap items-center gap-1.5">
              <Chip active={kindFilter === null} onClick={() => setKindFilter(null)}>
                All
                <span className="ml-1 tabular-nums opacity-60">{rows.length}</span>
              </Chip>
              {kinds.map(([kind, n]) => (
                <Chip
                  key={kind}
                  active={kindFilter === kind}
                  onClick={() => setKindFilter(kindFilter === kind ? null : kind)}
                >
                  {kind}
                  <span className="ml-1 tabular-nums opacity-60">{n}</span>
                </Chip>
              ))}
            </div>
          )}
        </div>
      )}

      {visible.length === 0 ? (
        <p className="rounded-lg border border-border border-dashed px-4 py-6 text-center text-muted-foreground text-sm">
          Nothing matches {query.trim() ? `“${query.trim()}”` : "that filter"}.
        </p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {visible.map((row) => {
            const { tone, icon, named } = styleFor(row.kind);
            return (
              <li
                key={row.id}
                data-testid="rule-row"
                className={[
                  "glass group flex items-center gap-3 rounded-xl border border-border px-3.5 py-2.5",
                  "transition-[border-color,background-color,transform] duration-150",
                  "hover:-translate-y-px hover:border-border-strong hover:bg-elevated/50",
                ].join(" ")}
              >
                <TargetIcon value={row.value || row.kind} glyph={named ? undefined : icon} />

                <p className="min-w-0 flex-1 truncate font-medium text-foreground text-sm">
                  {row.value || "—"}
                </p>

                <Badge tone={tone} icon={icon} outlined>
                  {row.kind}
                </Badge>

                <Button
                  variant="ghost"
                  tone="destructive"
                  size="icon"
                  // Revealed on hover so a long list is not a column of red bins.
                  className="opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100"
                  aria-label={`Remove ${row.value || row.kind}`}
                  onClick={() => onRemove(row.id)}
                >
                  <Trash2 />
                </Button>
              </li>
            );
          })}
        </ul>
      )}

      {filtered && visible.length > 0 && (
        <p className="mt-2 text-faint-foreground text-xs">
          Showing {visible.length} of {count(rows.length, noun)}.
        </p>
      )}
    </>
  );
}

function Chip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={[
        "rounded-full border px-2.5 py-1 text-xs transition-colors",
        active
          ? "border-primary/40 bg-primary-dim text-primary"
          : "border-border text-muted-foreground hover:border-border-strong hover:bg-hover hover:text-foreground",
      ].join(" ")}
    >
      {children}
    </button>
  );
}
