import {
  Ban,
  CircleCheck,
  ExternalLink,
  LoaderCircle,
  Minus,
  Plus,
  ShieldCheck,
  ShieldOff,
  TriangleAlert,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Select } from "@/components/Select";
import {
  addSite,
  type AppStatus,
  type BlockListSummary,
  fetchLists,
  fetchSiteStatus,
  fetchStatus,
  removeSite,
  type SiteStatus,
  showApp,
} from "@/lib/api";
import { send } from "@/lib/messages";
import { canonicalHost, isInternalUrl } from "@/lib/rules";

interface Snapshot {
  connected: boolean;
  status: AppStatus | null;
  lists: BlockListSummary[];
  ruleCount: number;
  blockEverything: boolean;
}

/** Rules these buttons can safely take back out again. */
const REMOVABLE = new Set(["domain", "wildcard"]);

export function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [host, setHost] = useState<string | null>(null);
  const [site, setSite] = useState<SiteStatus | null>(null);
  const [listId, setListId] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);

  const load = useCallback(
    async (currentHost: string | null) => {
      const [reply, status, lists, siteStatus] = await Promise.all([
        send({ type: "status" }),
        fetchStatus(),
        fetchLists(),
        currentHost ? fetchSiteStatus(currentHost) : Promise.resolve(null),
      ]);
      setSnapshot({
        connected: reply?.connected ?? false,
        status,
        lists: lists ?? [],
        ruleCount: reply?.ruleCount ?? 0,
        blockEverything: reply?.blockEverything ?? false,
      });
      setSite(siteStatus);
      setListId((current) => current || lists?.find((l) => l.enabled)?.id || lists?.[0]?.id || "");
    },
    [],
  );

  useEffect(() => {
    void (async () => {
      let current: string | null = null;
      try {
        const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
        const parsed = tab?.url ? new URL(tab.url) : null;
        if (parsed && !isInternalUrl(parsed.protocol)) current = canonicalHost(parsed.hostname);
      } catch {
        // No tab or an unparseable URL. The popup still works, it just cannot
        // offer to block the current site.
      }
      setHost(current);
      await load(current);
    })();
  }, [load]);

  async function block() {
    if (!host || !listId) return;
    setBusy(true);
    setNote(null);

    const ok = await addSite(listId, host);
    const name = snapshot?.lists.find((l) => l.id === listId)?.name ?? "the list";
    setNote({ ok, text: ok ? `Blocked ${host} in ${name}` : "Could not reach the desktop app" });

    if (ok) {
      await send({ type: "refresh" });
      await load(host);
    }
    setBusy(false);
  }

  /**
   * Removes the site from every list holding it, not from whichever list a
   * dropdown happened to be showing — that was why unblocking appeared to work
   * and then the site stayed blocked.
   */
  async function unblock() {
    if (!host || !site) return;
    setBusy(true);
    setNote(null);

    const removable = site.lists.filter((l) => REMOVABLE.has(l.rule_kind));
    const results = await Promise.all(removable.map((l) => removeSite(l.id, host)));

    const removed = results.reduce((n, r) => n + (r?.removed ?? 0), 0);
    const stubborn = site.lists.filter((l) => !REMOVABLE.has(l.rule_kind));

    if (results.some((r) => r === null)) {
      setNote({ ok: false, text: "Could not reach the desktop app" });
    } else if (removed === 0 && stubborn.length === 0) {
      setNote({ ok: false, text: `${host} was not in any list` });
    } else if (stubborn.length > 0) {
      setNote({
        ok: false,
        text: `Still matched by a ${stubborn[0]?.rule_kind.replace("_", " ")} rule in ${stubborn[0]?.name}. Edit it in the app.`,
      });
    } else {
      const names = removable.map((l) => l.name).join(", ");
      setNote({ ok: true, text: `Unblocked ${host} in ${names}` });
    }

    await send({ type: "refresh" });
    await load(host);
    setBusy(false);
  }

  if (!snapshot) {
    return (
      <div className="flex h-52 items-center justify-center bg-background">
        <LoaderCircle className="size-5 animate-spin text-faint-foreground" />
      </div>
    );
  }

  const { connected, status, lists, ruleCount, blockEverything } = snapshot;
  const summary = blockEverything
    ? "Blocking everything"
    : `${ruleCount} ${ruleCount === 1 ? "rule" : "rules"} active`;
  const accent = connected ? "rgb(139 92 246 / 0.6)" : "rgb(248 113 113 / 0.6)";
  const listed = site?.lists ?? [];

  return (
    <div className="relative flex flex-col overflow-hidden bg-background">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-0 h-40"
        style={{
          background: connected
            ? "radial-gradient(20rem 8rem at 50% 0%, rgb(139 92 246 / 0.22), transparent 70%)"
            : "radial-gradient(20rem 8rem at 50% 0%, rgb(248 113 113 / 0.16), transparent 70%)",
        }}
      />
      <div
        aria-hidden
        className="absolute inset-x-0 top-0 h-px"
        style={{
          background: `linear-gradient(90deg, transparent, ${accent} 30%, ${accent} 70%, transparent)`,
        }}
      />

      <header className="relative flex items-center gap-3 px-4 pt-4 pb-3.5">
        <div
          className={`flex size-9 items-center justify-center rounded-xl border ${
            connected
              ? "border-primary/35 bg-primary/15 text-primary"
              : "border-destructive/35 bg-destructive/15 text-destructive"
          }`}
        >
          {connected ? <ShieldCheck className="size-4" /> : <ShieldOff className="size-4" />}
        </div>
        <div className="min-w-0 flex-1">
          <p className="font-semibold text-foreground text-sm leading-tight">Focuser</p>
          <p className="truncate text-faint-foreground text-xs">
            {connected ? summary : "Desktop app not running"}
          </p>
        </div>
        <span
          className={`inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2 py-1 font-medium text-[0.65rem] uppercase tracking-wide ${
            connected
              ? "border-success/30 bg-success/10 text-success"
              : "border-destructive/30 bg-destructive/10 text-destructive"
          }`}
        >
          <span
            aria-hidden
            className={`size-1.5 rounded-full ${connected ? "bg-success" : "bg-destructive"}`}
          />
          {connected ? "On" : "Off"}
        </span>
      </header>

      {!connected && (
        <div className="relative mx-4 mb-4 flex items-start gap-2.5 rounded-xl border border-warning/25 bg-warning/10 px-3 py-2.5">
          <TriangleAlert className="mt-px size-4 shrink-0 text-warning" />
          <p className="text-muted-foreground text-xs leading-relaxed">
            Nothing is being blocked. Start the Focuser desktop app to restore your rules.
          </p>
        </div>
      )}

      {connected && status && (
        <div className="relative mx-4 mb-4 grid grid-cols-3 gap-2">
          <Stat value={status.blocked_today} label="Today" />
          <Stat value={status.blocked_sites} label="Sites" />
          <Stat value={status.active_lists} label="Lists" />
        </div>
      )}

      <section className="relative border-border border-t px-4 py-4">
        <p className="mb-2 font-medium text-[0.65rem] text-faint-foreground uppercase tracking-[0.12em]">
          Current site
        </p>

        <div className="rounded-xl border border-border bg-elevated/50 px-3 py-2.5">
          <p className="truncate font-medium text-foreground text-sm">{host ?? "No site open"}</p>
          {host && connected && (
            <p className="mt-1 flex items-center gap-1.5 text-xs">
              {listed.length > 0 ? (
                <>
                  <Ban className="size-3 shrink-0 text-destructive" />
                  <span className="truncate text-muted-foreground">
                    In {listed.map((l) => l.name).join(", ")}
                  </span>
                </>
              ) : (
                <>
                  <CircleCheck className="size-3 shrink-0 text-success" />
                  <span className="text-muted-foreground">Not in any list</span>
                </>
              )}
            </p>
          )}
        </div>

        {host && connected && (
          <>
            {listed.length > 0 ? (
              <button
                type="button"
                disabled={busy}
                onClick={() => void unblock()}
                className="mt-2.5 inline-flex w-full items-center justify-center gap-1.5 rounded-xl border border-border-strong bg-elevated px-3 py-2.5 font-medium text-foreground text-sm transition-colors hover:bg-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary disabled:pointer-events-none disabled:opacity-50"
              >
                <Minus className="size-4" />
                Unblock everywhere
              </button>
            ) : lists.length === 0 ? (
              <p className="mt-3 text-muted-foreground text-xs leading-relaxed">
                Create a block list in the desktop app first — there is nowhere to add this site
                yet.
              </p>
            ) : (
              <>
                <div className="mt-2.5">
                  <Select
                    ariaLabel="Block list"
                    value={listId}
                    onChange={setListId}
                    options={lists.map((l) => ({
                      value: l.id,
                      label: l.name,
                      hint: `${l.website_count} sites${l.enabled ? "" : " · off"}`,
                    }))}
                  />
                </div>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void block()}
                  className="mt-2.5 inline-flex w-full items-center justify-center gap-1.5 rounded-xl bg-primary px-3 py-2.5 font-medium text-primary-foreground text-sm shadow-[0_6px_18px_-8px_var(--color-primary)] transition-[transform,background-color] duration-200 hover:-translate-y-0.5 hover:bg-primary-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary active:translate-y-0 disabled:pointer-events-none disabled:opacity-50"
                >
                  <Plus className="size-4" />
                  Block this site
                </button>
              </>
            )}
          </>
        )}

        {note && (
          <p
            role="status"
            className={`mt-2.5 text-xs leading-relaxed ${note.ok ? "text-success" : "text-destructive"}`}
          >
            {note.text}
          </p>
        )}
      </section>

      <footer className="relative border-border border-t p-2">
        <button
          type="button"
          onClick={() => void showApp()}
          className="inline-flex w-full items-center justify-center gap-1.5 rounded-lg px-3 py-2 text-muted-foreground text-sm transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
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
    <div className="rounded-xl border border-border bg-elevated/50 px-2 py-2.5 text-center">
      <p className="font-semibold text-foreground text-lg tabular-nums leading-none">{value}</p>
      <p className="mt-1.5 text-[0.62rem] text-faint-foreground uppercase tracking-[0.1em]">
        {label}
      </p>
    </div>
  );
}
