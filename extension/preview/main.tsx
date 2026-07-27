/**
 * Renders the extension's pages in an ordinary browser tab.
 *
 * The block page and popup can otherwise only be seen by loading the built
 * extension into a real browser and getting yourself blocked, which is a slow
 * way to check a padding change. This harness has no `browser.*` access — it
 * is for looks only, not behaviour.
 *
 *   npm run preview   →   http://localhost:5199
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BlockPage } from "@/components/BlockPage";
import type { BlockContext } from "@/lib/messages";
import "@/assets/tailwind.css";

const CASES: Array<{ title: string; context: BlockContext }> = [
  {
    title: "First visit, social media",
    context: {
      hostname: "reddit.com",
      target: "reddit.com",
      reason: "domain",
      category: "social_media",
      count: 1,
    },
  },
  {
    title: "Persistent, video",
    context: {
      hostname: "youtube.com",
      target: "youtube.com",
      reason: "domain",
      category: "video",
      count: 14,
    },
  },
  {
    title: "Keyword match, gambling",
    context: {
      hostname: "some-site.example",
      target: "casino",
      reason: "keyword",
      category: "gambling",
      count: 5,
    },
  },
  {
    title: "Everything blocked",
    context: {
      hostname: "news.example",
      target: "news.example",
      reason: "everything",
      category: "default",
      count: 3,
    },
  },
];

function Preview() {
  return (
    <div className="bg-deep">
      {CASES.map(({ title, context }) => (
        <section key={title}>
          <p className="border-border border-y bg-surface px-6 py-2 font-mono text-faint-foreground text-xs">
            {title}
          </p>
          <BlockPage context={context} />
        </section>
      ))}
    </div>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <Preview />
    </StrictMode>,
  );
}
