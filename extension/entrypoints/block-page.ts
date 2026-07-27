import { createRoot } from "react-dom/client";
import { createElement } from "react";
import { BlockPage } from "@/components/BlockPage";
import type { BlockContext } from "@/lib/messages";
// Inlined as a string so the whole stylesheet can go inside the shadow root.
// A <link> would resolve against the blocked page's origin and be subject to
// its CSP, which many sites set strictly enough to break it.
import styles from "@/assets/tailwind.css?inline";

/**
 * Replaces a blocked page in place.
 *
 * Injected by the background worker rather than navigated to, so the address
 * bar still shows where the user tried to go — a redirect would lose that,
 * pollute history, and make "back" return to the blocked site.
 *
 * The UI lives in a shadow root: the page's own stylesheet cannot reach in and
 * ours cannot leak out, which matters when the "page" being styled is an
 * arbitrary site we just took over.
 */
export default defineUnlistedScript(() => {
  const raw = (window as unknown as Record<string, unknown>).__focuser;
  delete (window as unknown as Record<string, unknown>).__focuser;

  let context: BlockContext;
  try {
    context = JSON.parse(String(raw)) as BlockContext;
  } catch {
    // Injected without context — still block, just without the detail.
    context = {
      hostname: location.hostname,
      target: location.hostname,
      reason: "domain",
      category: "default",
      count: 1,
    };
  }

  // Stop whatever the page was doing: media, timers, pending scripts.
  window.stop?.();

  document.documentElement.replaceChildren(document.createElement("head"), document.createElement("body"));
  document.documentElement.style.cssText =
    "background:#0b0b11;visibility:visible;color-scheme:dark";
  // The replacement body is brand new, so it carries the UA's 8px margin. With
  // a `min-h-screen` layout inside, that margin is a scrollbar on every block
  // page — invisible in the preview, whose own HTML already zeroes it.
  document.body.style.cssText = "margin:0;padding:0;background:#0b0b11";
  document.title = `Blocked — ${context.hostname}`;

  const host = document.createElement("div");
  host.id = "focuser-block-root";
  document.body.appendChild(host);

  const shadow = host.attachShadow({ mode: "open" });
  const sheet = document.createElement("style");
  sheet.textContent = styles;
  shadow.appendChild(sheet);

  const mount = document.createElement("div");
  shadow.appendChild(mount);

  createRoot(mount).render(createElement(BlockPage, { context }));
});
