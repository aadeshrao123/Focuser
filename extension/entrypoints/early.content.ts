import { send } from "@/lib/messages";

/**
 * Runs at `document_start`, before the page paints.
 *
 * Without this there is a visible flash of the blocked site while the
 * background decides — which is exactly the moment of temptation the whole
 * product exists to remove. So the page is hidden first and revealed only once
 * the answer comes back "not blocked".
 *
 * Fails open on purpose: if the background never answers, the page is shown.
 * A blocker that leaves every tab blank when it breaks is worse than one that
 * occasionally misses.
 */
export default defineContentScript({
  matches: ["<all_urls>"],
  runAt: "document_start",
  allFrames: false,

  async main() {
    const style = document.createElement("style");
    style.textContent = "html{background:#0b0b11!important;visibility:hidden!important}";
    (document.head ?? document.documentElement).appendChild(style);

    // Belt and braces: if the background is slow or gone, reveal anyway.
    const failOpen = setTimeout(() => style.remove(), 2_000);

    const reply = await send({
      type: "check-url",
      hostname: location.hostname,
      url: location.href,
    });

    clearTimeout(failOpen);
    if (!reply?.blocked) style.remove();
    // When blocked, the style stays until the background replaces the document.
  },
});
