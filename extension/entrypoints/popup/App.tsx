import {
  ExternalLink,
  Loader2,
  Minus,
  Plus,
  ShieldCheck,
  ShieldOff,
  TriangleAlert,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  addSite,
  type AppStatus,
  type BlockListSummary,
  fetchLists,
  fetchStatus,
  removeSite,
  showApp,
} from "@/lib/api";
import { canonicalHost, isInternalUrl } from "@/lib/rules";
import { send } from "@/lib/messages";

interface Snapshot {
  connected: boolean;
  status: AppStatus | null;
  lists: BlockListSummary[];
  ruleCount: number;
  blockEverything: boolean;
}

export function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [host, setHost] = useState<string | null>(null);
  const [listId, setListId] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const load = useCallback(async () => {
    const [reply, status, lists] = await Promise.all([
      send({ type: "status" }),
      fetchStatus(),
      fetchLists(),
    ]);
    setSnapshot({
      connected: reply?.connected ?? false,
      status,
      lists: lists ?? [],
      ruleCount: reply?.ruleCount ?? 0,
      blockEverything: reply?.blockEverything ?? false,
    });
    setListId((current) => current || lists?.find((l) => l.enabled)?.id || lists?.[0]?.id || "");
  }, []);

  useEffect(() => {
    void load();
    void (async () => {
      const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
      if (!tab?.url) return;
      try {
        const parsed = new URL(tab.url);
        if (isInternalUrl(parsed.protocol)) return;
        setHost(canonicalHost(parsed.hostname));
      } catch {
        /* unparseable */
      }
    })();
  }, [load]);

  async function mutate(action: "add" | "remove") {
    if (!host || !listId) return;
    setBusy(true);
    setNote(null);
    const ok = await (action === "add" ? addSite(listId, host) : removeSite(listId, host));
    setNote(
      ok
        ? action === "add"
          ? `Blocked ${host}`
          : `Unblocked ${host}`
        : "Could not reach the app",
    );
    if (ok) {
      await send({ type: "refresh" });
      await load();
    }
    setBusy(false);
  }

  if (!snapshot) {
    return (
      <div className="flex h-40 items-center justify-center bg-background">
        <Loader2 className="size-5 animate-spin text-faint-foreground" />
      </div>
    );
  }

  const { connected, status, lists, ruleCount, blockEverything } = snapshot;

  return (
    <div className="flex flex-col bg-background">
      <header className="flex items-center gap-2.5 border-border border-b px-4 py-3.5">
        <div
          className={`flex size-8 items-center justify-center rounded-lg ${
            connected ? "bg-primary/15 text-primary" : "bg-destructive/15 text-destructive"
          }`}
        >
          {connected ? <ShieldCheck className="size-4" /> : <ShieldOff className="size-4" />}
        </div>
        <div className="min-w-0 flex-1">
          <p className="font-semibold text-foreground text-sm leading-tight">Focuser</p>
          <p className="truncate text-faint-foreground text-xs">
            {connected
              ? blockEverything
                ? "Blocking everything"
                : `${ruleCount} ${ruleCount === 1 ? "rule" : "rules"} active`
              : "Desktop app not running"}
          </p>
        </div>
      </header>

      {!connected && (
        <div className="flex items-start gap-2.5 border-warning/25 border-b bg-warning/10 px-4 py-3">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" />
          <p className="text-muted-foreground text-xs leading-relaxed">
            Nothing is being blocked. Start the Focuser desktop app to restore your rules.
          </p>
        </div>
      )}

      {connected && status && (
        <div className="grid grid-cols-3 divide-x divide-border border-border border-b">
          <Stat value={status.blocked_today} label="today" />
          <Stat value={status.blocked_sites} label="sites" />
          <Stat value={status.active_lists} label="lists" />
        </div>
      )}

      <section className="px-4 py-4">
        <p className="mb-1.5 text-faint-foreground text-xs uppercase tracking-wide">Current site</p>
        <p className="truncate font-medium text-foreground text-sm">{host ?? "No site open"}</p>

        {host && connected && (
          <>
            {lists.length === 0 ? (
              <p className="mt-3 text-muted-foreground text-xs">
                Create a block list in the app first.
              </p>
            ) : (
              <>
                <select
                  value={listId}
                  onChange={(e) => setListId(e.target.value)}
                  className="mt-3 w-full rounded-lg border border-border-strong bg-elevated px-3 py-2 text-foreground text-sm outline-none focus-visible:border-primary"
                >
                  {lists.map((list) => (
                    <option key={list.id} value={list.id}>
                      {list.name}
                      {list.enabled ? "" : " (off)"}
                    </option>
                  ))}
                </select>

                <div className="mt-2.5 flex gap-2">
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void mutate("add")}
                    className="inline-flex flex-1 items-center justify-center gap-1.5 rounded-lg bg-primary px-3 py-2 font-medium text-primary-foreground text-sm transition-colors hover:bg-primary-hover disabled:opacity-50"
                  >
                    <Plus className="size-4" />
                    Block
                  </button>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void mutate("remove")}
                    title={`Remove ${host} from this list`}
                    className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-border-strong bg-elevated px-3 py-2 font-medium text-foreground text-sm transition-colors hover:bg-hover disabled:opacity-50"
                  >
                    <Minus className="size-4" />
                    Unblock
                  </button>
                </div>
              </>
            )}
          </>
        )}

        {note && <p className="mt-2.5 text-muted-foreground text-xs">{note}</p>}
      </section>

      <footer className="border-border border-t px-4 py-3">
        <button
          type="button"
          onClick={() => void showApp()}
          className="inline-flex w-full items-center justify-center gap-1.5 rounded-lg px-3 py-2 text-muted-foreground text-sm transition-colors hover:bg-hover hover:text-foreground"
        >
          Open Focuser
          <ExternalLink className="size-3.5" />
        </button>
      </footer>
    </div>
  );
}

function Stat({ value, label }: { value: number; label: string }) {
  return (
    <div className="px-2 py-3 text-center">
      <p className="font-semibold text-foreground text-lg tabular-nums leading-none">{value}</p>
      <p className="mt-1 text-faint-foreground text-[0.7rem] uppercase tracking-wide">{label}</p>
    </div>
  );
}
