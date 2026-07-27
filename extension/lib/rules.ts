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
  blocked_domains: string[];
  blocked_keywords: string[];
  blocked_wildcards: string[];
  blocked_url_paths: string[];
  block_entire_internet: boolean;
  allowed_domains: string[];
  domain_categories?: Record<string, string>;
  version?: number;
}

/** Rules with the hot paths pre-canonicalised, built once per rules update. */
export interface CompiledRules {
  domains: Set<string>;
  keywords: string[];
  wildcards: string[];
  urlPaths: string[];
  blockEverything: boolean;
  allowed: Set<string>;
  categories: Record<string, string>;
}

export const EMPTY_RULES: CompiledRules = {
  domains: new Set(),
  keywords: [],
  wildcards: [],
  urlPaths: [],
  blockEverything: false,
  allowed: new Set(),
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
  let host = String(raw ?? "")
    .trim()
    .toLowerCase();

  const scheme = host.indexOf("://");
  if (scheme !== -1) host = host.slice(scheme + 3);

  const at = host.indexOf("@");
  if (at !== -1) host = host.slice(at + 1);

  host = host.split(/[/?#]/)[0] ?? "";

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

export function compile(rules: RuleSet | null): CompiledRules {
  if (!rules) return EMPTY_RULES;
  return {
    domains: canonicalSet(rules.blocked_domains),
    keywords: (rules.blocked_keywords ?? []).map((k) => k.toLowerCase()),
    wildcards: rules.blocked_wildcards ?? [],
    urlPaths: (rules.blocked_url_paths ?? []).map((p) => p.toLowerCase()),
    blockEverything: rules.block_entire_internet ?? false,
    allowed: canonicalSet(rules.allowed_domains),
    categories: rules.domain_categories ?? {},
  };
}

export function isAllowed(rules: CompiledRules, hostname: string): boolean {
  return setCovers(rules.allowed, hostname);
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
export function match(
  rules: CompiledRules,
  hostname: string,
  url: string,
): BlockMatch | null {
  const host = canonicalHost(hostname);
  const lowerUrl = (url ?? "").toLowerCase();

  if (isAllowed(rules, host)) return null;
  if (rules.blockEverything) return { reason: "everything", target: host };
  if (setCovers(rules.domains, host)) return { reason: "domain", target: host };

  for (const keyword of rules.keywords) {
    if (lowerUrl.includes(keyword)) return { reason: "keyword", target: keyword };
  }
  for (const path of rules.urlPaths) {
    if (lowerUrl.includes(path)) return { reason: "url-path", target: path };
  }
  for (const raw of rules.wildcards) {
    const pattern = raw.toLowerCase();
    if (matchWildcard(pattern, host) || matchWildcard(pattern, lowerUrl)) {
      return { reason: "wildcard", target: raw };
    }
  }
  return null;
}

export function isBlocked(
  rules: CompiledRules,
  hostname: string,
  url: string,
): boolean {
  return match(rules, hostname, url) !== null;
}

/** Pages the extension must never touch: its own, and the browser's. */
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
    rules.domains.size +
    rules.keywords.length +
    rules.urlPaths.length +
    rules.wildcards.length
  );
}
