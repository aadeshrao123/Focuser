import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fakeBrowser } from "wxt/testing/fake-browser";
import background from "@/entrypoints/background";
import { profileId } from "./api";
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

/** Every script the worker put into a page. The block page is one of them. */
let injected = vi.fn(async (_injection: { files?: string[] }) => undefined);
const blockPages = () => injected.mock.calls.filter(([i]) => i.files?.includes("/block-page.js"));

/** Start the background worker with `rules` already loaded. */
async function start(
  rules: RuleSet = RULES,
  {
    appRunning = true,
    channel = "",
  }: { appRunning?: boolean; channel?: string | ((asked: string) => string) } = {},
) {
  const fetched = vi.fn(async (url: RequestInfo | URL, _init?: RequestInit) => {
    // YouTube's oEmbed answer: which channel a video is from.
    if (String(url).startsWith("https://www.youtube.com/oembed")) {
      const author = typeof channel === "function" ? channel(String(url)) : channel;
      return new Response(JSON.stringify({ author_url: author }));
    }
    if (!appRunning) throw new TypeError("Failed to fetch");
    return String(url).includes("/api/rules")
      ? new Response(JSON.stringify(rules))
      : new Response("{}");
  });
  vi.stubGlobal("fetch", fetched);

  // fake-browser has no toolbar button, script injection or idle detection.
  // "idle" is someone watching a video: the screen is on, the hands are not.
  const noop = vi.fn(async () => undefined);
  const badge = vi.fn(async () => undefined);
  injected = vi.fn(async (_injection: { files?: string[] }) => undefined);
  Object.assign(fakeBrowser, {
    action: { setBadgeText: badge, setBadgeBackgroundColor: noop, setTitle: noop },
    scripting: { executeScript: injected },
    idle: { queryState: vi.fn(async () => "idle"), onStateChanged: { addListener: noop } },
  });

  background.main();
  // The badge is set once the first rules fetch has been applied.
  if (appRunning) await vi.waitFor(() => expect(badge).toHaveBeenCalled());
  return fetched;
}

beforeEach(() => {
  fakeBrowser.reset();
  // reset() leaves the navigation listeners behind, and every test starts its
  // own worker. Left there, an earlier worker would answer these events too.
  fakeBrowser.webNavigation.onCommitted.removeAllListeners();
  fakeBrowser.webNavigation.onHistoryStateUpdated.removeAllListeners();
});
afterEach(() => vi.restoreAllMocks());

describe("moving between pages without a page load", () => {
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

  it("leaves a tab alone that already shows the block page", async () => {
    const reload = vi.spyOn(fakeBrowser.tabs, "reload").mockResolvedValue();
    await start();

    // The site's scripts still run under the block page, and YouTube rewrites
    // its URL as it starts. Reloading for that brought the block back, which
    // the site rewrote again: a reload loop.
    await fakeBrowser.webNavigation.onCommitted.trigger({
      tabId: 7,
      frameId: 0,
      url: "https://www.youtube.com/watch?v=abc",
    } as never);
    await fakeBrowser.webNavigation.onHistoryStateUpdated.trigger({
      tabId: 7,
      frameId: 0,
      url: "https://www.youtube.com/watch?v=abc&t=0",
    } as never);
    expect(reload).not.toHaveBeenCalled();
  });

  it("leaves a tab alone while its block page is still on the way", async () => {
    const reload = vi.spyOn(fakeBrowser.tabs, "reload").mockResolvedValue();
    const fetched = await start();

    // The block page waits on the app for its count, and the site runs in
    // the meantime. A URL rewrite in that gap reloaded the tab.
    let answer: (r: Response) => void = () => {};
    fetched.mockImplementation(
      (url: RequestInfo | URL) =>
        new Promise((resolve) => {
          if (String(url).includes("/api/blocked")) answer = resolve;
          else resolve(new Response("{}"));
        }),
    );
    const committed = fakeBrowser.webNavigation.onCommitted.trigger({
      tabId: 7,
      frameId: 0,
      url: "https://www.youtube.com/watch?v=abc",
    } as never);
    await fakeBrowser.webNavigation.onHistoryStateUpdated.trigger({
      tabId: 7,
      frameId: 0,
      url: "https://www.youtube.com/watch?v=abc&t=0",
    } as never);
    answer(new Response(JSON.stringify({ count: 1 })));
    await committed;

    expect(reload).not.toHaveBeenCalled();
    expect(blockPages()).toHaveLength(1);
  });
});

describe("telling the app about private windows", () => {
  /** The first heartbeat a freshly started worker sent, as a parsed address. */
  async function firstHeartbeat() {
    const fetched = await start();
    const beat = fetched.mock.calls
      .map(([url]) => new URL(String(url)))
      .find((url) => url.pathname === "/api/heartbeat");
    if (!beat) throw new Error("the worker sent no heartbeat");
    return beat;
  }
  const privateWindows = (answer: () => Promise<unknown>) =>
    Object.assign(fakeBrowser, { extension: { isAllowedIncognitoAccess: answer } });

  it("says no when the browser keeps it out of them, and which profile is talking", async () => {
    privateWindows(async () => false);
    const beat = await firstHeartbeat();

    expect(beat.searchParams.get("incognito_allowed")).toBe("false");
    // The app holds each profile to its own answer, so it needs to tell them
    // apart. Two profiles of one browser are two copies of this extension.
    expect(beat.searchParams.get("profile")).toMatch(/^[0-9a-f-]{36}$/);
  });

  it("keeps its id from one start of the worker to the next", async () => {
    privateWindows(async () => true);
    const first = await firstHeartbeat();
    const second = await firstHeartbeat();

    expect(second.searchParams.get("profile")).toBe(first.searchParams.get("profile"));
  });

  it("says nothing when it cannot find out", async () => {
    // "I do not know" must not read as "no": the app closes the browser on no.
    for (const answer of [
      async () => {
        throw new Error("not in this browser");
      },
      async () => undefined,
    ]) {
      privateWindows(answer);
      const beat = await firstHeartbeat();

      expect(beat.searchParams.get("browser")).toBeTruthy();
      expect(beat.searchParams.has("incognito_allowed")).toBe(false);
    }
  });
});

describe("telling the app when the extension is removed", () => {
  const removalAddress = () =>
    vi.spyOn(fakeBrowser.runtime, "setUninstallURL").mockResolvedValue(undefined);

  it("leaves the browser an address of the app's to open, with this profile's id", async () => {
    const left = removalAddress();
    await start();

    await vi.waitFor(() => expect(left).toHaveBeenCalled());
    const url = new URL(String(left.mock.lastCall?.[0]));
    expect(`${url.origin}${url.pathname}`).toBe("http://127.0.0.1:17549/api/extension-removed");
    expect(url.searchParams.get("browser")).toBeTruthy();
    // The id the heartbeat sends. A web page can open the address too, but
    // it cannot know this.
    expect(url.searchParams.get("profile")).toBe(await profileId());
  });

  it("leaves none while the app is not running, where the tab would only show an error", async () => {
    const left = removalAddress();
    await start(RULES, { appRunning: false });

    await vi.waitFor(() => expect(left).toHaveBeenCalledWith(""));
  });
});

describe("the browser's extensions page while a list is locked", () => {
  const LOCKED: RuleSet = { ...RULES, locked: true };
  const privateWindows = (allowed: boolean) =>
    Object.assign(fakeBrowser, { extension: { isAllowedIncognitoAccess: async () => allowed } });
  /** Open the extensions page in tab 5 and return where the worker sent it. */
  async function openExtensionsPage() {
    const sent = vi.spyOn(fakeBrowser.tabs, "update").mockResolvedValue({} as never);
    await fakeBrowser.tabs.onUpdated.trigger(5, { url: "chrome://extensions/" }, {} as never);
    await new Promise((resolve) => setTimeout(resolve, 0));
    return sent;
  }

  it("sends it to a page that says why", async () => {
    privateWindows(true);
    await start(LOCKED);
    const sent = await openExtensionsPage();

    await vi.waitFor(() => expect(sent).toHaveBeenCalled());
    expect(sent.mock.lastCall?.[0]).toBe(5);
    expect(String(sent.mock.lastCall?.[1]?.url)).toContain("/locked.html");
  });

  it("leaves it open while nothing is locked", async () => {
    privateWindows(true);
    await start(RULES);
    expect(await openExtensionsPage()).not.toHaveBeenCalled();
  });

  it("leaves it open while private windows are not allowed, since that is fixed there", async () => {
    privateWindows(false);
    await start(LOCKED);
    expect(await openExtensionsPage()).not.toHaveBeenCalled();
  });
});

describe("a video from a blocked YouTube channel (#48)", () => {
  const CHANNEL: RuleSet = {
    ...RULES,
    blocked_domains: [],
    allowed_url_paths: [],
    blocked_url_paths: ["youtube.com/@badchannel"],
  };
  const visit = (url: string) =>
    fakeBrowser.webNavigation.onCommitted.trigger({ tabId: 7, frameId: 0, url } as never);
  const move = (url: string) =>
    fakeBrowser.webNavigation.onHistoryStateUpdated.trigger({ tabId: 7, frameId: 0, url } as never);

  it("gets the block page", async () => {
    await start(CHANNEL, { channel: "https://www.youtube.com/@BadChannel" });
    await visit("https://www.youtube.com/watch?v=vid00000101");
    await vi.waitFor(() => expect(blockPages()).toHaveLength(1));
  });

  it("does not when the video is from another channel", async () => {
    const fetched = await start(CHANNEL, { channel: "https://www.youtube.com/@GoodChannel" });
    await visit("https://www.youtube.com/watch?v=vid00000102");
    await vi.waitFor(() =>
      expect(fetched.mock.calls.some(([u]) => String(u).includes("/oembed"))).toBe(true),
    );
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(blockPages()).toHaveLength(0);
  });

  it("is not looked up when no rule names a channel", async () => {
    const fetched = await start({ ...CHANNEL, blocked_url_paths: [] });
    await visit("https://www.youtube.com/watch?v=vid00000103");
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(fetched.mock.calls.some(([u]) => String(u).includes("/oembed"))).toBe(false);
  });

  it("reloads when the page moves on to such a video", async () => {
    const reload = vi.spyOn(fakeBrowser.tabs, "reload").mockResolvedValue();
    await start(CHANNEL, {
      channel: (asked) =>
        asked.includes("bad")
          ? "https://www.youtube.com/@BadChannel"
          : "https://www.youtube.com/@GoodChannel",
    });

    await visit("https://www.youtube.com/watch?v=goodvid0001");
    await move("https://www.youtube.com/watch?v=goodvid0001&t=1");
    await move("https://www.youtube.com/watch?v=badvid00001");
    await vi.waitFor(() => expect(reload).toHaveBeenCalledWith(7));
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("does not reload while a blocked video only rewrites its own address", async () => {
    // YouTube rewrites the address of the video it has just loaded. A reload
    // for that would bring the block back and start the loop from #36 again.
    const reload = vi.spyOn(fakeBrowser.tabs, "reload").mockResolvedValue();
    await start(CHANNEL, { channel: "https://www.youtube.com/@BadChannel" });

    await visit("https://www.youtube.com/watch?v=badvid00002");
    await move("https://www.youtube.com/watch?v=badvid00002&t=1");
    await vi.waitFor(() => expect(blockPages()).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(reload).not.toHaveBeenCalled();
  });
});

describe("opening a blocked site", () => {
  const visit = (url: string, frameId = 0) =>
    fakeBrowser.webNavigation.onCommitted.trigger({ tabId: 7, frameId, url } as never);

  it("puts the block page into every new page, however soon the next one comes", async () => {
    await start();

    // The same site twice in a row, the way a quick reload does it. The second
    // page used to get nothing: it was taken for a repeat of the first.
    await visit("https://www.youtube.com/watch?v=abc");
    await visit("https://www.youtube.com/watch?v=abc");
    expect(blockPages()).toHaveLength(2);

    // A frame inside the page is not a new page.
    await visit("https://www.youtube.com/embed/abc", 3);
    expect(blockPages()).toHaveLength(2);
  });

  // #63: Firefox keeps extensions out of its PDF viewer, and the viewer
  // rewrites its address as the reader scrolls. Each rewrite reloaded the tab.
  it("does not reload a page the block page cannot go into", async () => {
    const reload = vi.spyOn(fakeBrowser.tabs, "reload").mockResolvedValue();
    await start({ ...RULES, block_entire_internet: true });
    injected.mockRejectedValue(new Error("Missing host permission for the tab"));

    await visit("https://example.test/paper.pdf");
    await fakeBrowser.webNavigation.onHistoryStateUpdated.trigger({
      tabId: 7,
      frameId: 0,
      url: "https://example.test/paper.pdf#page=2",
    } as never);
    expect(reload).not.toHaveBeenCalled();
  });

  it("leaves a file on the computer alone with the whole internet blocked", async () => {
    const reload = vi.spyOn(fakeBrowser.tabs, "reload").mockResolvedValue();
    await start({ ...RULES, block_entire_internet: true });

    await visit("file:///C:/Users/me/Downloads/paper.pdf");
    await fakeBrowser.webNavigation.onHistoryStateUpdated.trigger({
      tabId: 7,
      frameId: 0,
      url: "file:///C:/Users/me/Downloads/paper.pdf#page=2",
    } as never);
    expect(blockPages()).toHaveLength(0);
    expect(reload).not.toHaveBeenCalled();
  });
});

describe("time on a page under a shared allowance", () => {
  const VIDEO = "https://www.youtube.com/watch?v=abc";
  /** The app sends scopes only while a shared allowance is running. */
  const SHARED: RuleSet = {
    ...RULES,
    scopes: [{ rules: RULES, shared_permits: true, scheduled: true }],
  };

  /** Five seconds on the video, in the front tab of the focused window. */
  async function watch(rules: RuleSet) {
    const clock = vi.spyOn(Date, "now").mockReturnValue(1_000_000);
    vi.spyOn(fakeBrowser.windows, "getLastFocused").mockResolvedValue({ focused: true } as never);
    vi.spyOn(fakeBrowser.tabs, "query").mockResolvedValue([{ id: 7, url: VIDEO }] as never);
    const fetched = await start(rules);

    const ticks = () =>
      fetched.mock.calls
        .filter(([url]) => String(url).includes("/api/allowance-tick"))
        .map(([, init]) => JSON.parse(String(init?.body)));

    clock.mockReturnValue(1_005_000);
    await fakeBrowser.windows.onFocusChanged.trigger(1);
    // The normal allowance tick goes out on the same event, scopes or not.
    await vi.waitFor(() => expect(ticks().some((t) => !t.shared_only)).toBe(true));
    return ticks;
  }

  it("is counted while a video plays with no input", async () => {
    const ticks = await watch(SHARED);

    await vi.waitFor(() =>
      expect(ticks().filter((t) => t.shared_only)).toEqual([
        expect.objectContaining({ url: VIDEO, increment_secs: 5, shared_active: true }),
      ]),
    );
  });

  it("is not reported to an app that sent no scopes", async () => {
    // An app from before shared allowances reads such a tick as a normal one
    // and takes the time off the site's own allowance a second time.
    const ticks = await watch({ ...RULES, blocked_domains: ["reddit.com"] });

    expect(ticks().filter((t) => t.shared_only)).toEqual([]);
  });
});
