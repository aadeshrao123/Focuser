/**
 * Rule matching. Pure functions over a compiled rule set — no browser APIs, so
 * every branch is testable without a extension harness.
 *
 * The desktop app compiles block lists into this shape and serves it at
 * `/api/rules`. Keyword, wildcard and URL-path rules exist here and *only*
 * here: a hosts file cannot express them, which is the whole reason the
 * extension is the preferred blocking path.
 */

/** The rule set exactly as the desktop app serves it. */
export interface RuleSet {
  scopes?: { rules: RuleSet; shared_permits: boolean | null; scheduled: boolean }[];
  blocked_domains: string[];
  blocked_keywords: string[];
  blocked_wildcards: string[];
  blocked_url_paths: string[];
  block_entire_internet: boolean;
  allowed_domains: string[];
  allowed_wildcards?: string[];
  /** Pages allowed on a site that is otherwise blocked, as `host/path`. */
  allowed_url_paths?: string[];
  /** Sites an allowance is keeping open. Sent with `scopes`. */
  allowance_domains?: string[];
  domain_categories?: Record<string, string>;
  version?: number;
  /** A block list is locked right now. */
  locked?: boolean;
}

/** Rules with the hot paths pre-canonicalised, built once per rules update. */
export interface CompiledRules {
  scopes?: { rules: CompiledRules; sharedPermits: boolean | null; scheduled: boolean }[];
  domains: Set<string>;
  keywords: string[];
  wildcards: string[];
  urlPaths: string[];
  blockEverything: boolean;
  allowed: Set<string>;
  allowedWildcards: string[];
  allowedPages: AllowedPage[];
  allowances: Set<string>;
  categories: Record<string, string>;
}

/** One allowed page: its canonical host and its lower-cased path. */
export interface AllowedPage {
  host: string;
  page: string;
}

export const EMPTY_RULES: CompiledRules = {
  domains: new Set(),
  keywords: [],
  wildcards: [],
  urlPaths: [],
  blockEverything: false,
  allowed: new Set(),
  allowedWildcards: [],
  allowedPages: [],
  allowances: new Set(),
  categories: {},
};

/**
 * Reduce anything host-shaped to one comparable form.
 *
 * Accepts a bare host, a full URL, or something with credentials, a port or a
 * trailing dot. `www.` is stripped so a rule for `example.com` and a visit to
 * `www.example.com` are the same thing — the desktop app makes the same
 * assumption, and the two must agree or a rule silently misses.
 */
export function canonicalHost(raw: string | null | undefined): string {
  const lowered = String(raw ?? "")
    .trim()
    .toLowerCase();

  // A scheme only counts at the very start, and the path goes before the
  // credentials: an `@` after the first slash is part of the page
  // (youtube.com/@name). The Rust side parses it the same way.
  let host = lowered.replace(/^[a-z0-9+-]+:\/\//, "").split(/[/?#]/)[0] ?? "";
  host = host.slice(host.lastIndexOf("@") + 1);

  const colon = host.lastIndexOf(":");
  if (colon > 0) host = host.slice(0, colon);

  host = host.replace(/\.+$/, "");
  return host.startsWith("www.") ? host.slice(4) : host;
}

/**
 * Whether a set of rule hosts covers this host — exactly, or as a parent.
 *
 * A rule for `example.com` covers `mail.example.com`. It must not cover
 * `notexample.com`, which is why this walks label boundaries rather than
 * testing string suffixes.
 */
export function setCovers(set: Set<string>, host: string): boolean {
  const canonical = canonicalHost(host);
  if (!canonical) return false;
  if (set.has(canonical)) return true;

  const parts = canonical.split(".");
  for (let i = 1; i < parts.length; i++) {
    if (set.has(parts.slice(i).join("."))) return true;
  }
  return false;
}

export function canonicalSet(list: string[] | undefined): Set<string> {
  const set = new Set<string>();
  for (const entry of list ?? []) {
    const host = canonicalHost(entry);
    if (host) set.add(host);
  }
  return set;
}

/** Glob match. `*` is any run, `?` is one character. */
export function matchWildcard(pattern: string, value: string): boolean {
  const escaped = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".");
  try {
    return new RegExp(`^${escaped}$`).test(value);
  } catch {
    // A pattern that will not compile blocks nothing rather than everything.
    return false;
  }
}

/**
 * A wildcard against a hostname. `*.youtube.com` covers `youtube.com` too.
 *
 * People write the `*.` form meaning "the site", and a pattern that skipped the
 * apex just looked broken. `focuser_common::host::wildcard_matches` is the same
 * rule on the Rust side; the two have to agree or a rule silently misses.
 */
export function matchHostWildcard(pattern: string, hostname: string): boolean {
  const canonical = canonicalHost(hostname);
  if (!canonical) return false;

  // Both the host as given and its canonical form: a glob may be aiming at the
  // `www.` label that canonicalHost removes.
  const raw = String(hostname ?? "")
    .trim()
    .toLowerCase();
  const glob = pattern.trim().toLowerCase();
  if (matchWildcard(glob, raw) || matchWildcard(glob, canonical)) return true;

  const apex = glob.startsWith("*.") ? glob.slice(2) : "";
  if (!apex || /[*?]/.test(apex)) return false;
  return setCovers(new Set([apex]), canonical);
}

export function compile(rules: RuleSet | null): CompiledRules {
  if (!rules) return EMPTY_RULES;
  return {
    scopes: rules.scopes?.map((s) => ({
      rules: compile(s.rules),
      sharedPermits: s.shared_permits,
      scheduled: s.scheduled,
    })),
    domains: canonicalSet(rules.blocked_domains),
    keywords: (rules.blocked_keywords ?? []).map((k) => k.toLowerCase()),
    wildcards: rules.blocked_wildcards ?? [],
    urlPaths: (rules.blocked_url_paths ?? []).map((p) => p.toLowerCase()),
    blockEverything: rules.block_entire_internet ?? false,
    allowed: canonicalSet(rules.allowed_domains),
    allowedWildcards: rules.allowed_wildcards ?? [],
    allowedPages: (rules.allowed_url_paths ?? []).flatMap(compilePage),
    allowances: canonicalSet(rules.allowance_domains),
    categories: rules.domain_categories ?? {},
  };
}

/** `host/path` as the app sends it. An entry with no path is dropped. */
function compilePage(entry: string): AllowedPage[] {
  const slash = entry.indexOf("/");
  const host = canonicalHost(entry);
  if (slash === -1 || !host) return [];
  return [{ host, page: entry.slice(slash).toLowerCase() }];
}

/**
 * On the allowed page or under it. `/r/programming` covers
 * `/r/programming/comments` but not `/r/programminghumor`, so the match has
 * to end on a boundary.
 */
function onPage(allowed: AllowedPage, hostname: string, url: string): boolean {
  const host = canonicalHost(hostname);
  if (host !== allowed.host && !host.endsWith(`.${allowed.host}`)) return false;

  let path: string;
  try {
    const parsed = new URL(url);
    path = (parsed.pathname + parsed.search).toLowerCase();
  } catch {
    return false;
  }
  if (!path.startsWith(allowed.page)) return false;
  const next = path[allowed.page.length];
  return next === undefined || "/?&".includes(next);
}

export function isAllowed(rules: CompiledRules, hostname: string, url = ""): boolean {
  if (setCovers(rules.allowed, hostname)) return true;
  if (rules.allowedWildcards.some((pattern) => matchHostWildcard(pattern, hostname))) return true;
  return rules.allowedPages.some((page) => onPage(page, hostname, url));
}

/**
 * Why a page was blocked. `null` means it wasn't.
 *
 * The caller needs the *reason*, not just a boolean — the block page shows the
 * matched keyword rather than the hostname when a keyword is what caught it,
 * and statistics are keyed on it.
 */
export type BlockMatch =
  | { reason: "everything"; target: string }
  | { reason: "domain"; target: string }
  | { reason: "keyword"; target: string }
  | { reason: "url-path"; target: string }
  | { reason: "wildcard"; target: string };

/**
 * Decide whether a navigation is blocked, and by what.
 *
 * Exceptions win over every rule, including "block the entire internet" —
 * an allowance that cannot be honoured is not an allowance.
 */
export function match(rules: CompiledRules, hostname: string, url: string): BlockMatch | null {
  // Only web pages. A file opened from the computer is not the internet, even
  // with all of it blocked (#63).
  if (!/^https?:/i.test(url ?? "")) return null;
  const host = canonicalHost(hostname);
  const lowerUrl = (url ?? "").toLowerCase();

  // With scopes, each list brings its own exceptions. The flat ones are every
  // list's mixed together, for an extension older than this one, so only the
  // allowances are read from the top level here.
  if (rules.scopes?.length) {
    const applicable = rules.scopes
      .map((s) => ({ ...s, hit: match(s.rules, hostname, url) }))
      .filter((s) => s.hit);
    const shared = applicable.some((s) => s.sharedPermits !== null);
    for (const scope of applicable) {
      if (scope.sharedPermits === true) continue;
      if (!shared && !scope.scheduled && setCovers(rules.allowances, hostname)) continue;
      return scope.hit;
    }
    return null;
  }

  if (isAllowed(rules, hostname, url)) return null;
  if (rules.blockEverything) return { reason: "everything", target: host };
  if (setCovers(rules.domains, host)) return { reason: "domain", target: host };

  for (const keyword of rules.keywords) {
    if (lowerUrl.includes(keyword)) return { reason: "keyword", target: keyword };
  }
  for (const path of rules.urlPaths) {
    if (lowerUrl.includes(path)) return { reason: "url-path", target: path };
  }
  const pages = pageForms(lowerUrl);
  for (const raw of rules.wildcards) {
    const glob = raw.toLowerCase();
    if (matchHostWildcard(raw, hostname) || pages.some((page) => matchWildcard(glob, page))) {
      return { reason: "wildcard", target: raw };
    }
  }
  return null;
}

/**
 * The address as given, then without `https://`, then without `www.` too.
 * People write a page pattern the way they would type it, like
 * `youtube.com/@name*`, and that has neither (#48).
 */
function pageForms(url: string): string[] {
  const bare = url.replace(/^[a-z][a-z0-9+.-]*:\/\//, "");
  return [url, bare, bare.replace(/^www\./, "")];
}

export function isBlocked(rules: CompiledRules, hostname: string, url: string): boolean {
  return match(rules, hostname, url) !== null;
}

/** Pages the extension must never touch: its own, and the browser's. */
/**
 * The browser's own page for its extensions, where this one can be switched
 * off. Kept shut while a block list is locked.
 */
export function isExtensionsPage(url: string): boolean {
  return (
    /^(chrome|edge|brave|opera|vivaldi):\/\/extensions(\/|\?|$)/i.test(url) ||
    /^about:addons\b/i.test(url)
  );
}

export function isInternalUrl(protocol: string): boolean {
  return (
    protocol === "chrome:" ||
    protocol === "chrome-extension:" ||
    protocol === "about:" ||
    protocol === "moz-extension:" ||
    protocol === "edge:" ||
    protocol === "data:" ||
    protocol === "view-source:"
  );
}

/**
 * How the block is counted in statistics.
 *
 * Keyword matches are keyed by the keyword, not the host, so "blocked 40 times
 * by the word `casino`" stays one row instead of forty different domains.
 */
export function trackingKey(hit: BlockMatch): string {
  return hit.reason === "keyword" || hit.reason === "wildcard" || hit.reason === "url-path"
    ? `kw:${hit.target.toLowerCase()}`
    : hit.target.toLowerCase();
}

/** How many distinct things the current rules block, for the toolbar badge. */
export function ruleCount(rules: CompiledRules): number {
  return (
    rules.domains.size + rules.keywords.length + rules.urlPaths.length + rules.wildcards.length
  );
}
