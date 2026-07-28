/**
 * Client for the desktop app's local HTTP API.
 *
 * Everything stays on the machine: the app serves this on loopback only, and
 * the extension talks to nothing else. There is no remote endpoint anywhere in
 * this codebase, by design.
 *
 * The old extension also tried Native Messaging first and fell back to HTTP
 * after three failures. The native host has had nothing to connect to since the
 * standalone service was removed, so that path only ever cost startup latency.
 * HTTP is the only transport now.
 */

import type { RuleSet } from "./rules";

export const API_BASE = "http://127.0.0.1:17549";

/** How often to re-read rules. The app recompiles them on every change. */
export const POLL_INTERVAL_MS = 2_000;

/** Which browser we are, for the app's "extension connected" tracking. */
export type BrowserName = "Chrome" | "Firefox" | "Edge" | "Brave" | "Opera" | "Other";

export function detectBrowser(): BrowserName {
  const ua = navigator.userAgent;
  // Order matters: Brave, Edge and Opera all claim to be Chrome.
  if ("brave" in navigator) return "Brave";
  if (ua.includes("Edg/")) return "Edge";
  if (ua.includes("OPR/") || ua.includes("Opera")) return "Opera";
  if (ua.includes("Firefox/")) return "Firefox";
  if (ua.includes("Chrome/")) return "Chrome";
  return "Other";
}

/** Result of a rules fetch. `null` means the app is not reachable. */
export async function fetchRules(browser: BrowserName): Promise<RuleSet | null> {
  try {
    const response = await fetch(`${API_BASE}/api/rules`, {
      headers: { "X-Focuser-Browser": browser },
    });
    if (!response.ok) return null;
    return (await response.json()) as RuleSet;
  } catch {
    // The app being closed is a normal state, not an error worth logging on
    // every poll — the toolbar badge is how the user finds out.
    return null;
  }
}

/**
 * Tell the app we are alive, so it does not close the browser for running
 * without the extension.
 */
export async function sendHeartbeat(browser: BrowserName): Promise<void> {
  try {
    await fetch(`${API_BASE}/api/heartbeat?browser=${encodeURIComponent(browser)}`);
  } catch {
    /* app closed */
  }
}

/**
 * Record a block and get back how many times this target has been blocked.
 *
 * The count is shown on the block page. A failed report still returns 1 rather
 * than throwing, because a statistics hiccup must never stop the page being
 * blocked.
 */
export async function reportBlocked(
  domain: string,
  key: string,
): Promise<number> {
  try {
    const response = await fetch(`${API_BASE}/api/blocked`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ domain, tracking_key: key }),
    });
    const data = (await response.json()) as { count?: number };
    return data?.count ?? 1;
  } catch {
    return 1;
  }
}

/**
 * Report time spent on the active tab, so website allowances count down.
 *
 * `increment_secs` is clamped: a service worker that slept for an hour must not
 * retroactively bill the user an hour of quota when it wakes.
 */
export async function sendAllowanceTick(
  hostname: string,
  incrementSecs: number,
  source: string,
): Promise<void> {
  try {
    await fetch(`${API_BASE}/api/allowance-tick`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        hostname,
        app_exe: null,
        active: true,
        source,
        increment_secs: incrementSecs,
      }),
    });
  } catch {
    /* app closed */
  }
}

/** Clamp an elapsed span to a sane per-tick credit. */
export function clampIncrement(elapsedMs: number): number {
  return Math.min(60, Math.max(5, Math.round(elapsedMs / 1000)));
}

// ─── Popup-facing endpoints ──────────────────────────────────────────

export interface AppStatus {
  running: boolean;
  active_lists: number;
  blocked_sites: number;
  blocked_apps: number;
  blocked_today: number;
}

export interface BlockListSummary {
  id: string;
  name: string;
  enabled: boolean;
  website_count: number;
  app_count: number;
}

async function getJson<T>(path: string): Promise<T | null> {
  try {
    const response = await fetch(`${API_BASE}${path}`);
    if (!response.ok) return null;
    return (await response.json()) as T;
  } catch {
    return null;
  }
}

async function postJson<T = unknown>(path: string, body: unknown): Promise<T | null> {
  try {
    const response = await fetch(`${API_BASE}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!response.ok) return null;
    return (await response.json().catch(() => ({}))) as T;
  } catch {
    return null;
  }
}

/** Where a site is listed, which is not the same as which list is selected. */
export interface SiteStatus {
  domain: string;
  blocked: boolean;
  lists: Array<{
    id: string;
    name: string;
    enabled: boolean;
    rule_kind: "domain" | "wildcard" | "keyword" | "url_path" | "everything";
  }>;
}

export const fetchStatus = () => getJson<AppStatus>("/api/status");
export const fetchLists = () => getJson<BlockListSummary[]>("/api/lists");

export const fetchSiteStatus = (domain: string) =>
  getJson<SiteStatus>(`/api/site-status?domain=${encodeURIComponent(domain)}`);

export const addSite = (listId: string, domain: string) =>
  postJson("/api/add-site", { list_id: listId, domain, rule_type: "domain" }).then(
    (r) => r !== null,
  );

/**
 * Removes the site from one list, reporting what actually happened.
 *
 * `removed` is the point: the API used to answer "ok" whether or not anything
 * matched, so unblocking a site held by a *different* list looked like it had
 * worked while the site stayed blocked.
 */
export async function removeSite(
  listId: string,
  domain: string,
): Promise<{ removed: number; leftBehind: number } | null> {
  const reply = await postJson<{ removed?: number; left_behind?: number }>("/api/remove-site", {
    list_id: listId,
    domain,
  });
  if (!reply) return null;
  return { removed: reply.removed ?? 0, leftBehind: reply.left_behind ?? 0 };
}

/** Ask the desktop app to bring its window to the front. */
export const showApp = () => postJson("/api/show", {}).then((r) => r !== null);
