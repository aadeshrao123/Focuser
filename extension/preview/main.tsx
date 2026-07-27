/**
 * Renders the extension's pages in an ordinary browser tab.
 *
 * The block page and popup can otherwise only be seen by loading the built
 * extension into a real browser and getting yourself blocked, which is a slow
 * way to check a padding change.
 *
 * One state at a time, at full height: an earlier version stacked every case
 * down one scrolling page, which is not how anyone ever sees this.
 *
 *   npm run preview   →   http://localhost:5199
 */
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { BlockPage } from "@/components/BlockPage";
import { App as Popup } from "@/entrypoints/popup/App";
import type { BlockContext } from "@/lib/messages";
import "@/assets/tailwind.css";

// ─── Runtime stubs ───────────────────────────────────────────────────
// The popup talks to the extension runtime and to the desktop app on
// 127.0.0.1. Neither exists in an ordinary tab, so the harness fakes both.
// Kept here rather than behind a prop on the component: production code should
// not know it is being previewed.

type PopupMode = "connected" | "disconnected" | "no-lists";
let popupMode: PopupMode = "connected";

const LISTS = [
  { id: "l1", name: "Deep work", enabled: true },
  { id: "l2", name: "Evenings", enabled: false },
];

Object.assign(globalThis, {
  browser: {
    tabs: { query: async () => [{ url: "https://www.reddit.com/r/all" }] },
    runtime: {
      sendMessage: async () => ({
        type: "status",
        connected: popupMode !== "disconnected",
        rules: null,
        ruleCount: 23,
        blockEverything: false,
      }),
    },
  },
});

const realFetch = globalThis.fetch.bind(globalThis);
globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
  const url = String(input instanceof Request ? input.url : input);
  if (!url.includes("17549")) return realFetch(input as RequestInfo, init);
  if (popupMode === "disconnected") throw new Error("preview: app not running");

  const body = url.includes("/api/lists")
    ? popupMode === "no-lists"
      ? []
      : LISTS
    : { blocked_today: 47, blocked_sites: 12, active_lists: 2 };
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}) as typeof fetch;

// ─── Cases ───────────────────────────────────────────────────────────

const BLOCK_CASES: Array<{ title: string; context: BlockContext }> = [
  {
    title: "First visit",
    context: {
      hostname: "reddit.com",
      target: "reddit.com",
      reason: "domain",
      category: "social_media",
      count: 1,
    },
  },
  {
    title: "Persistent",
    context: {
      hostname: "youtube.com",
      target: "youtube.com",
      reason: "domain",
      category: "videos",
      count: 14,
    },
  },
  {
    title: "Keyword",
    context: {
      hostname: "lucky-spin-palace.example",
      target: "casino",
      reason: "keyword",
      category: "gambling",
      count: 5,
    },
  },
  {
    title: "Everything",
    context: {
      hostname: "news.ycombinator.com",
      target: "news.ycombinator.com",
      reason: "everything",
      category: "news",
      count: 3,
    },
  },
  {
    title: "Adult",
    context: {
      hostname: "some-adult-site.example",
      target: "some-adult-site.example",
      reason: "domain",
      category: "adult",
      count: 2,
    },
  },
  {
    // The layout breaker: a hostname with nothing to wrap on, a long matched
    // target, and a count in the top message tier.
    title: "Long host",
    context: {
      hostname: "an-extremely-long-subdomain.another-segment.example-domain-name.co.uk",
      target: "an-extremely-long-keyword-that-matched",
      reason: "url-path",
      category: "shopping",
      count: 137,
    },
  },
];

const POPUP_CASES: Array<{ title: string; mode: PopupMode }> = [
  { title: "Popup", mode: "connected" },
  { title: "Popup · off", mode: "disconnected" },
  { title: "Popup · no lists", mode: "no-lists" },
];

function Preview() {
  const [active, setActive] = useState(0);
  const popupIndex = active - BLOCK_CASES.length;
  const popup = POPUP_CASES[popupIndex];
  if (popup) popupMode = popup.mode;

  return (
    <>
      <nav className="fixed top-3 left-1/2 z-50 flex -translate-x-1/2 flex-wrap justify-center gap-1 rounded-full border border-border-strong bg-surface/90 p-1 shadow-[var(--shadow-depth-md)] backdrop-blur-xl">
        {[...BLOCK_CASES, ...POPUP_CASES].map((c, i) => (
          <button
            key={c.title}
            type="button"
            onClick={() => setActive(i)}
            className={`rounded-full px-3 py-1.5 font-medium text-xs transition-colors ${
              i === active
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:bg-hover hover:text-foreground"
            }`}
          >
            {c.title}
          </button>
        ))}
      </nav>

      {popup ? (
        <div className="flex min-h-screen items-center justify-center bg-deep p-6">
          {/* Framed at the real popup width — judging a 21rem panel while it
              stretches across a desktop window tells you nothing. */}
          <div className="w-[21rem] overflow-hidden rounded-xl border border-border-strong shadow-[var(--shadow-depth-lg)]">
            <Popup key={popup.mode} />
          </div>
        </div>
      ) : (
        (() => {
          const chosen = BLOCK_CASES[active] ?? BLOCK_CASES[0];
          return chosen ? <BlockPage key={chosen.title} context={chosen.context} /> : null;
        })()
      )}
    </>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <Preview />
    </StrictMode>,
  );
}
