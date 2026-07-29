/**
 * The preview itself. Imported dynamically by `main.tsx` so the browser stubs
 * are already in place — see the note at the top of `stubs.ts`.
 */
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { BlockPage } from "@/components/BlockPage";
import { App as Popup } from "@/entrypoints/popup/App";
import { Welcome } from "@/entrypoints/welcome/Welcome";
import type { BlockContext } from "@/lib/messages";
import { LOCALE_NAMES, type PopupMode, setLocale, setPopupMode } from "./stubs";
import "@/assets/tailwind.css";

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

// Welcome reads `?reason` off the URL to decide whether it is greeting a new
// install or explaining an update, so the harness sets it rather than passing a
// prop the real page does not have.
const WELCOME_CASES = [
  { title: "Welcome", reason: "install" },
  { title: "Welcome · update", reason: "update" },
];

function Preview() {
  const [active, setActive] = useState(0);
  const [language, setLanguage] = useState("en");
  setLocale(language);

  const popup = POPUP_CASES[active - BLOCK_CASES.length];
  if (popup) setPopupMode(popup.mode);

  const welcome = WELCOME_CASES[active - BLOCK_CASES.length - POPUP_CASES.length];
  if (welcome) {
    const url = new URL(window.location.href);
    url.searchParams.set("reason", welcome.reason);
    window.history.replaceState(null, "", url);
  }

  return (
    <>
      <nav className="fixed top-3 left-1/2 z-50 flex -translate-x-1/2 flex-wrap items-center justify-center gap-1 rounded-full border border-border-strong bg-surface/90 p-1 shadow-[var(--shadow-depth-md)] backdrop-blur-xl">
        {[...BLOCK_CASES, ...POPUP_CASES, ...WELCOME_CASES].map((c, i) => (
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

        <select
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
          aria-label="Language"
          className="rounded-full bg-hover px-3 py-1.5 font-medium text-foreground text-xs"
        >
          {LOCALE_NAMES.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
      </nav>

      {welcome ? (
        <Welcome key={`${welcome.reason}-${language}`} />
      ) : popup ? (
        <div className="flex min-h-screen items-center justify-center bg-deep p-6">
          {/* Framed at the real popup width — judging a 21rem panel while it
              stretches across a desktop window tells you nothing. */}
          <div className="w-[21rem] overflow-hidden rounded-xl border border-border-strong shadow-[var(--shadow-depth-lg)]">
            {/* Language is in the key so switching remounts. A real user only
                ever sees these pages built from scratch in one language. */}
            <Popup key={`${popup.mode}-${language}`} />
          </div>
        </div>
      ) : (
        (() => {
          const chosen = BLOCK_CASES[active] ?? BLOCK_CASES[0];
          return chosen ? (
            <BlockPage key={`${chosen.title}-${language}`} context={chosen.context} />
          ) : null;
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
