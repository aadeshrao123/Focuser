import { beforeEach, describe, expect, it, vi } from "vitest";
import { fakeBrowser } from "wxt/testing/fake-browser";
import background from "@/entrypoints/background";
import type { RuleSet } from "./rules";

const RULES: RuleSet = {
  blocked_domains: ["youtube.com"],
  blocked_keywords: [],
  blocked_wildcards: [],
  blocked_url_paths: [],
  block_entire_internet: false,
  allowed_domains: [],
  allowed_wildcards: [],
  allowed_url_paths: ["youtube.com/@YouTube"],
};

/** Start the background worker with `RULES` already loaded. */
async function start() {
  const fetched = vi.fn(async (url: RequestInfo | URL) =>
    String(url).includes("/api/rules")
      ? new Response(JSON.stringify(RULES))
      : new Response("{}"),
  );
  vi.stubGlobal("fetch", fetched);

  // fake-browser has no toolbar button, script injection or idle detection.
  const noop = vi.fn(async () => undefined);
  const badge = vi.fn(async () => undefined);
  Object.assign(fakeBrowser, {
    action: { setBadgeText: badge, setBadgeBackgroundColor: noop, setTitle: noop },
    scripting: { executeScript: noop },
    idle: { queryState: noop, onStateChanged: { addListener: noop } },
  });

  background.main();
  // The badge is set once the first rules fetch has been applied.
  await vi.waitFor(() => expect(badge).toHaveBeenCalled());
}

describe("moving between pages without a page load", () => {
  beforeEach(() => fakeBrowser.reset());

  it("reloads the tab when an allowed page leads to a blocked one", async () => {
    const reload = vi.spyOn(fakeBrowser.tabs, "reload").mockResolvedValue();
    await start();

    const move = (url: string, frameId = 0) =>
      fakeBrowser.webNavigation.onHistoryStateUpdated.trigger({
        tabId: 7,
        frameId,
        url,
      } as never);

    // #21: YouTube changes page in place, so the allowed channel page was a
    // door to every video on the site.
    await move("https://www.youtube.com/@YouTube/videos");
    await move("https://www.youtube.com/watch?v=abc", 3);
    expect(reload).not.toHaveBeenCalled();

    await move("https://www.youtube.com/watch?v=abc");
    expect(reload).toHaveBeenCalledWith(7);
  });
});
