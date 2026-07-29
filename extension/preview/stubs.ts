/**
 * Everything the extension expects from a browser that an ordinary tab does not
 * have: `browser.i18n`, `browser.tabs`, `browser.runtime`, and a desktop app
 * answering on 127.0.0.1.
 *
 * **This has to run before any extension module is imported.** `@wxt-dev/browser`
 * resolves `globalThis.browser` once, at import time, and falls back to
 * `globalThis.chrome` — which does not exist here — if `runtime.id` is missing.
 * So the stub needs both an `id` and a head start. `main.tsx` calls `install()`
 * and then imports the app dynamically; that ordering is the whole reason this
 * file is separate.
 *
 * Kept out of the components themselves: production code should not know it is
 * being previewed.
 */
import { load } from "js-yaml";
import pkg from "../package.json";

export type PopupMode = "connected" | "disconnected" | "no-lists";

let popupMode: PopupMode = "connected";
let locale = "en";

export function setPopupMode(mode: PopupMode) {
  popupMode = mode;
}

export function setLocale(name: string) {
  locale = name;
}

// ─── Locales ─────────────────────────────────────────────────────────
// Read from the YAML source rather than the built `_locales`, so a translator
// sees an edit on save instead of after a rebuild.

type YamlNode = string | number | { [key: string]: YamlNode };

const YAML = import.meta.glob("../locales/*.yml", {
  query: "?raw",
  import: "default",
  eager: true,
});

/**
 * `{ block: { goBack: "..." } }` becomes `{ block_goBack: "..." }`, and a
 * `1:`/`n:` pair joins with a pipe. That is what WXT compiles to and what
 * `@wxt-dev/i18n` splits back apart.
 */
function flatten(node: YamlNode, prefix = ""): Record<string, string> {
  if (typeof node !== "object" || node === null) return { [prefix]: String(node) };

  const keys = Object.keys(node);
  if (keys.every((k) => /^\d+$|^n$/.test(k))) {
    const order = keys.sort((a, b) => (a === "n" ? 1 : b === "n" ? -1 : Number(a) - Number(b)));
    return { [prefix]: order.map((k) => String(node[k])).join(" | ") };
  }

  const out: Record<string, string> = {};
  for (const key of keys) {
    Object.assign(out, flatten(node[key] as YamlNode, prefix ? `${prefix}_${key}` : key));
  }
  return out;
}

const LOCALES: Record<string, Record<string, string>> = {};
for (const [path, raw] of Object.entries(YAML)) {
  const name = path.split("/").pop()?.replace(/\.yml$/, "");
  if (name) LOCALES[name] = flatten(load(raw as string) as YamlNode);
}

export const LOCALE_NAMES = Object.keys(LOCALES).sort();

// ─── The stubs ───────────────────────────────────────────────────────

const LISTS = [
  { id: "l1", name: "Deep work", enabled: true },
  { id: "l2", name: "Evenings", enabled: false },
];

export function install() {
  Object.assign(globalThis, {
    browser: {
      i18n: {
        // Chrome's own contract: hand back the raw string and fill `$1`, `$2`.
        // Splitting plurals and substituting `{named}` is the library's job.
        getMessage: (key: string, subs?: string | string[]) => {
          const message = LOCALES[locale]?.[key] ?? LOCALES.en?.[key] ?? "";
          const list = subs === undefined ? [] : Array.isArray(subs) ? subs : [subs];
          return message.replace(/\$(\d)/g, (whole, n) => list[Number(n) - 1] ?? whole);
        },
        getUILanguage: () => locale,
      },
      tabs: { query: async () => [{ url: "https://www.reddit.com/r/all" }] },
      runtime: {
        // Present so `@wxt-dev/browser` picks this object over `globalThis.chrome`.
        id: "focuser-preview",
        // Read rather than hardcoded: the welcome page prints this, and a stale
        // number here means store screenshots taken from the preview are wrong.
        getManifest: () => ({ version: pkg.version }),
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
}
