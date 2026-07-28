import { describe, expect, it } from "vitest";
import {
  canonicalHost,
  compile,
  isInternalUrl,
  match,
  matchHostWildcard,
  matchWildcard,
  ruleCount,
  type RuleSet,
  setCovers,
  trackingKey,
} from "./rules";

function rules(patch: Partial<RuleSet> = {}) {
  return compile({
    blocked_domains: [],
    blocked_keywords: [],
    blocked_wildcards: [],
    blocked_url_paths: [],
    block_entire_internet: false,
    allowed_domains: [],
    allowed_wildcards: [],
    ...patch,
  });
}

describe("canonicalHost", () => {
  it("reduces every spelling of a host to one form", () => {
    for (const input of [
      "Example.com",
      "www.example.com",
      "https://example.com/path?q=1",
      "https://user:pw@example.com:8443/x",
      "example.com.",
      "  EXAMPLE.com  ",
    ]) {
      expect(canonicalHost(input), input).toBe("example.com");
    }
  });

  it("keeps subdomains that are not www", () => {
    expect(canonicalHost("mail.example.com")).toBe("mail.example.com");
  });

  it("survives nothing at all", () => {
    expect(canonicalHost("")).toBe("");
    expect(canonicalHost(null)).toBe("");
    expect(canonicalHost(undefined)).toBe("");
  });
});

describe("setCovers", () => {
  const set = new Set(["example.com"]);

  it("covers the host itself and its subdomains", () => {
    expect(setCovers(set, "example.com")).toBe(true);
    expect(setCovers(set, "mail.example.com")).toBe(true);
    expect(setCovers(set, "a.b.c.example.com")).toBe(true);
  });

  it("does not cover a host that merely ends with the same letters", () => {
    // The bug a naive `endsWith` would introduce: notexample.com is a
    // different site and must stay reachable.
    expect(setCovers(set, "notexample.com")).toBe(false);
    expect(setCovers(set, "example.com.evil.test")).toBe(false);
  });
});

describe("match", () => {
  it("blocks a domain rule and its subdomains", () => {
    const r = rules({ blocked_domains: ["reddit.com"] });
    expect(match(r, "reddit.com", "https://reddit.com/")).toEqual({
      reason: "domain",
      target: "reddit.com",
    });
    expect(match(r, "old.reddit.com", "https://old.reddit.com/")).not.toBeNull();
  });

  it("leaves everything else alone", () => {
    const r = rules({ blocked_domains: ["reddit.com"] });
    expect(match(r, "wikipedia.org", "https://wikipedia.org/")).toBeNull();
  });

  it("reports the keyword that matched, not the host", () => {
    // The block page shows this, and statistics key on it.
    const r = rules({ blocked_keywords: ["casino"] });
    expect(match(r, "example.com", "https://example.com/casino/play")).toEqual({
      reason: "keyword",
      target: "casino",
    });
  });

  it("matches wildcards against both host and full URL", () => {
    expect(
      match(rules({ blocked_wildcards: ["*.tracker.test"] }), "a.tracker.test", "https://a.tracker.test/"),
    ).not.toBeNull();
    expect(
      match(rules({ blocked_wildcards: ["*/watch?v=*"] }), "video.test", "https://video.test/watch?v=abc"),
    ).not.toBeNull();
  });

  it("matches url paths", () => {
    const r = rules({ blocked_url_paths: ["/shorts/"] });
    expect(match(r, "video.test", "https://video.test/shorts/xyz")?.reason).toBe("url-path");
    expect(match(r, "video.test", "https://video.test/watch")).toBeNull();
  });

  it("lets an exception beat a block", () => {
    const r = rules({ blocked_domains: ["example.com"], allowed_domains: ["docs.example.com"] });
    expect(match(r, "example.com", "https://example.com/")).not.toBeNull();
    expect(match(r, "docs.example.com", "https://docs.example.com/")).toBeNull();
  });

  it("lets an exception beat even block-the-entire-internet", () => {
    // An allowance that cannot be honoured is not an allowance. Without this,
    // a nuclear block would strand the user with no way back to the app.
    const r = rules({ block_entire_internet: true, allowed_domains: ["intranet.test"] });
    expect(match(r, "anything.test", "https://anything.test/")).not.toBeNull();
    expect(match(r, "intranet.test", "https://intranet.test/")).toBeNull();
  });

  it("treats a domain rule as beating a keyword when both apply", () => {
    const r = rules({ blocked_domains: ["casino.test"], blocked_keywords: ["casino"] });
    expect(match(r, "casino.test", "https://casino.test/")?.reason).toBe("domain");
  });
});

describe("matchHostWildcard", () => {
  it("covers the apex as well as subdomains", () => {
    for (const host of ["youtube.com", "www.youtube.com", "music.youtube.com"]) {
      expect(matchHostWildcard("*.youtube.com", host)).toBe(true);
    }
    expect(matchHostWildcard("*.youtube.com", "notyoutube.com")).toBe(false);
  });

  it("matches every host on a bare star", () => {
    expect(matchHostWildcard("*", "example.com")).toBe(true);
    expect(matchHostWildcard("*", "")).toBe(false);
  });
});

describe("wildcard exceptions", () => {
  // The app has always sent allowed_wildcards; the extension used to drop them
  // on the floor, so a wildcard exception released nothing.
  it("releases a host the blocked rules would otherwise catch", () => {
    const r = rules({
      blocked_domains: ["example.com"],
      allowed_wildcards: ["*.docs.example.com"],
    });
    expect(match(r, "docs.example.com", "https://docs.example.com/")).toBeNull();
    expect(match(r, "example.com", "https://example.com/")).not.toBeNull();
  });

  it("beats blocking the entire internet", () => {
    const r = rules({ block_entire_internet: true, allowed_wildcards: ["*.work.test"] });
    expect(match(r, "mail.work.test", "https://mail.work.test/")).toBeNull();
    expect(match(r, "reddit.com", "https://reddit.com/")).not.toBeNull();
  });
});

describe("matchWildcard", () => {
  it("treats * as any run and ? as one character", () => {
    expect(matchWildcard("*.example.com", "mail.example.com")).toBe(true);
    expect(matchWildcard("site?.test", "site1.test")).toBe(true);
    expect(matchWildcard("site?.test", "site12.test")).toBe(false);
  });

  it("does not let regex metacharacters in a pattern run wild", () => {
    // A user typing "a.b" means a literal dot, not "any character".
    expect(matchWildcard("a.b", "axb")).toBe(false);
    expect(matchWildcard("a.b", "a.b")).toBe(true);
  });

  it("blocks nothing when a pattern cannot compile", () => {
    // Failing open is wrong for a blocker in general, but a broken pattern
    // matching *everything* would take the whole browser down.
    expect(matchWildcard("[", "anything")).toBe(false);
  });
});

describe("trackingKey", () => {
  it("keys keyword blocks by the keyword so they aggregate", () => {
    expect(trackingKey({ reason: "keyword", target: "Casino" })).toBe("kw:casino");
  });

  it("keys domain blocks by the host", () => {
    expect(trackingKey({ reason: "domain", target: "Reddit.com" })).toBe("reddit.com");
  });
});

describe("isInternalUrl", () => {
  it("recognises browser and extension pages", () => {
    for (const p of ["chrome:", "chrome-extension:", "about:", "moz-extension:", "edge:", "data:"]) {
      expect(isInternalUrl(p), p).toBe(true);
    }
  });

  it("leaves the real web alone", () => {
    expect(isInternalUrl("https:")).toBe(false);
    expect(isInternalUrl("http:")).toBe(false);
  });
});

describe("ruleCount", () => {
  it("counts every kind of rule", () => {
    const r = rules({
      blocked_domains: ["a.test", "b.test"],
      blocked_keywords: ["x"],
      blocked_url_paths: ["/y/"],
      blocked_wildcards: ["*.z.test"],
    });
    expect(ruleCount(r)).toBe(5);
  });

  it("deduplicates domains that canonicalise to the same host", () => {
    expect(ruleCount(rules({ blocked_domains: ["example.com", "www.example.com"] }))).toBe(1);
  });
});
