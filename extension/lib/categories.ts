/**
 * Turning a blocked host into a category, and a category into something worth
 * reading on the block page.
 *
 * The messages escalate: the first time you hit a site today you get a gentle
 * nudge, the tenth time you get something blunter. A single fixed string stops
 * being read after the second visit, which makes the whole page decoration.
 */

import messages from "./category-messages.json";

export interface CategoryTier {
  min: number;
  max: number;
  messages: string[];
}

export interface CategoryInfo {
  label: string;
  icon: string;
  color: string;
  tiers: CategoryTier[];
}

interface MessageFile {
  categories: Record<string, CategoryInfo>;
  category_aliases?: Record<string, string>;
}

const data = messages as MessageFile;

export const CATEGORIES = data.categories;
const ALIASES = data.category_aliases ?? {};

export const DEFAULT_CATEGORY = "default";

/** Resolve an alias to the category that actually carries messages. */
export function resolveCategory(name: string): string {
  const key = name.toLowerCase();
  const aliased = ALIASES[key] ?? key;
  return CATEGORIES[aliased] ? aliased : DEFAULT_CATEGORY;
}

export function categoryInfo(name: string): CategoryInfo {
  const info = CATEGORIES[resolveCategory(name)];
  // The default category is present in the data file; the fallback exists so a
  // malformed file degrades to a working page rather than a blank one.
  return (
    info ?? {
      label: "Blocked",
      icon: "shield",
      color: "#8b5cf6",
      tiers: [{ min: 1, max: Number.MAX_SAFE_INTEGER, messages: ["This site is blocked."] }],
    }
  );
}

/**
 * The domain → category index built from the app's starter lists.
 *
 * Keys prefixed `wc:` are wildcard patterns rather than hosts; they are held
 * separately so a plain host lookup stays a hash hit.
 */
export interface CategoryIndex {
  hosts: Record<string, string>;
  wildcards: Array<{ pattern: string; category: string }>;
}

export const EMPTY_INDEX: CategoryIndex = { hosts: {}, wildcards: [] };

/** `premade-lists.json` as the desktop app ships it. */
interface PremadeFile {
  version?: number;
  categories?: Record<
    string,
    { name?: string; description?: string; domains?: string[]; wildcards?: string[] }
  >;
}

/**
 * Build the host → category index from `premade-lists.json`.
 *
 * The file is keyed by category id (`dating`, `social_media`, …), and those ids
 * have to survive [`resolveCategory`] to reach a message set. When they don't,
 * the block page silently falls back to generic text — which is what happened
 * to `videos` for a while, because the alias map only had `video`.
 */
export function buildIndex(raw: unknown): CategoryIndex {
  const index: CategoryIndex = { hosts: {}, wildcards: [] };
  const file = (raw ?? {}) as PremadeFile;

  for (const [id, list] of Object.entries(file.categories ?? {})) {
    const category = resolveCategory(id);

    for (const domain of list?.domains ?? []) {
      const host = String(domain).trim().toLowerCase().replace(/^www\./, "");
      if (host) index.hosts[host] = category;
    }
    for (const pattern of list?.wildcards ?? []) {
      const glob = String(pattern).trim().toLowerCase();
      if (glob) index.wildcards.push({ pattern: glob, category });
    }
  }
  return index;
}

/** Category for a host: exact, then each parent domain, then wildcards. */
export function categoryForHost(index: CategoryIndex, hostname: string): string {
  const host = hostname.toLowerCase().replace(/^www\./, "");
  const direct = index.hosts[host];
  if (direct) return direct;

  const parts = host.split(".");
  for (let i = 1; i < parts.length; i++) {
    const parent = index.hosts[parts.slice(i).join(".")];
    if (parent) return parent;
  }

  for (const { pattern, category } of index.wildcards) {
    if (host.includes(pattern.replace(/\*/g, ""))) return category;
  }
  return DEFAULT_CATEGORY;
}

/** Category for a keyword rule — the best guess from the starter lists. */
export function categoryForKeyword(index: CategoryIndex, keyword: string): string {
  const needle = keyword.toLowerCase();
  for (const [host, category] of Object.entries(index.hosts)) {
    if (host.includes(needle)) return category;
  }
  for (const { pattern, category } of index.wildcards) {
    if (pattern.includes(needle)) return category;
  }
  return DEFAULT_CATEGORY;
}

/**
 * Pick a message for this category at this visit count.
 *
 * Deterministic for a given (category, count) so a page that re-renders does
 * not shuffle its text under the reader.
 *
 * Roughly 40% of the messages carry `{count}` or `{domain}` placeholders, so
 * substitution is not optional — skipping it puts a literal `{domain}` in
 * front of the user.
 */
export function messageFor(category: string, count: number, domain = ""): string {
  const info = categoryInfo(category);
  const n = Math.max(1, count);
  const tier =
    info.tiers.find((t) => n >= t.min && n <= t.max) ??
    info.tiers[info.tiers.length - 1];

  const pool = tier?.messages ?? [];
  const template = pool[n % pool.length] ?? pool[0] ?? "This site is blocked.";
  return interpolate(template, n, domain);
}

/** Fill `{count}` and `{domain}`; drop any placeholder we cannot fill. */
function interpolate(template: string, count: number, domain: string): string {
  return template
    .replace(/\{count\}/g, String(count))
    .replace(/\{domain\}/g, domain || "this site")
    .replace(/\{\w+\}/g, "");
}
