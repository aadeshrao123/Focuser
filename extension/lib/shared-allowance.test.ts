import { expect, it } from "vitest";
import { compile, match, type RuleSet } from "./rules";
import { SharedActivity } from "./shared-activity";

const base = (): RuleSet => ({
  blocked_domains: [],
  blocked_keywords: [],
  blocked_wildcards: [],
  blocked_url_paths: [],
  block_entire_internet: false,
  allowed_domains: [],
});
const scope = (shared: boolean | null, domain = "youtube.com") => ({
  rules: { ...base(), blocked_domains: [domain] },
  shared_permits: shared,
  scheduled: true,
});
const hit = (
  scopes: NonNullable<RuleSet["scopes"]>,
  host = "youtube.com",
  allowed: string[] = [],
) => match(compile({ ...base(), scopes, allowed_domains: allowed }), host, `https://${host}/watch`);

it("shared budget permits access only within its own list", () => {
  expect(hit([scope(true)])).toBeNull();
  expect(hit([scope(true), scope(null)])).not.toBeNull();
  expect(hit([scope(true), scope(null, "other.com")])).toBeNull();
});
it("exhaustion has no fallback to an individual allowance", () => {
  expect(hit([scope(false)], "youtube.com", ["youtube.com"])).not.toBeNull();
});
it("multiple shared budgets never stack", () => {
  expect(hit([scope(true), scope(true)])).toBeNull();
  expect(hit([scope(true), scope(false)])).not.toBeNull();
  expect(hit([scope(false), scope(true)])).not.toBeNull();
});
it("individual allowance continues for unrelated targets", () => {
  const normal = scope(null, "reddit.com");
  normal.scheduled = false;
  expect(hit([scope(true), normal], "reddit.com", ["reddit.com"])).toBeNull();
  normal.scheduled = true;
  expect(hit([scope(true), normal], "reddit.com", ["reddit.com"])).not.toBeNull();
});
it("a parent-domain individual allowance cannot bypass a shared subdomain", () => {
  expect(
    hit([scope(false, "video.example.com")], "video.example.com", ["example.com"]),
  ).not.toBeNull();
});
it("one list's exception cannot exempt another list", () => {
  const excepted = scope(true);
  excepted.rules.allowed_domains = ["youtube.com"];
  expect(hit([excepted, scope(null)])).not.toBeNull();
});
it("path rules only consume their scoped target", () => {
  const s = scope(false);
  s.rules.blocked_domains = [];
  s.rules.blocked_url_paths = ["youtube.com/shorts"];
  expect(hit([s])).toBeNull();
  expect(
    match(compile({ ...base(), scopes: [s] }), "youtube.com", "https://youtube.com/shorts/1"),
  ).not.toBeNull();
});
it("samples focused usage without charging startup, tab switches, idle, or sleep", () => {
  const tracker = new SharedActivity();
  expect(tracker.sample("https://youtube.com/", 1000)).toBeNull();
  expect(tracker.sample("https://youtube.com/", 6000)).toEqual({
    url: "https://youtube.com/",
    seconds: 5,
  });
  expect(tracker.sample("https://reddit.com/", 8000)).toEqual({
    url: "https://youtube.com/",
    seconds: 2,
  });
  expect(tracker.sample(null, 10000)).toEqual({ url: "https://reddit.com/", seconds: 2 });
  expect(tracker.sample("https://reddit.com/", 12000)).toBeNull();
  expect(tracker.sample("https://reddit.com/", 100000)).toBeNull();
});
