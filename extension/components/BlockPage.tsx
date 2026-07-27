import {
  ArrowLeft,
  Ban,
  Gamepad2,
  Globe,
  Heart,
  Newspaper,
  Play,
  ShieldAlert,
  ShoppingBag,
  Spade,
  Users,
} from "lucide-react";
import type { ComponentType } from "react";
import { categoryInfo, messageFor, resolveCategory } from "@/lib/categories";
import type { BlockContext } from "@/lib/messages";

/**
 * Icons per category. Keyed on the resolved category rather than the icon name
 * in the data file, so a new category without an icon still renders.
 */
const ICONS: Record<string, ComponentType<{ className?: string }>> = {
  adult: ShieldAlert,
  social_media: Users,
  video: Play,
  dating: Heart,
  gaming: Gamepad2,
  news: Newspaper,
  shopping: ShoppingBag,
  gambling: Spade,
  default: Ban,
};

const REASON_LABEL: Record<BlockContext["reason"], string> = {
  domain: "This site is on a block list",
  keyword: "The address contains a blocked word",
  "url-path": "This part of the site is blocked",
  wildcard: "This address matches a blocked pattern",
  everything: "Everything is blocked right now",
};

function ordinal(n: number): string {
  const suffix =
    n % 100 >= 11 && n % 100 <= 13 ? "th" : ["th", "st", "nd", "rd"][n % 10] ?? "th";
  return `${n}${suffix}`;
}

export function BlockPage({ context }: { context: BlockContext }) {
  const category = resolveCategory(context.category);
  const info = categoryInfo(category);
  const Icon = ICONS[category] ?? ICONS.default ?? Ban;
  const message = messageFor(category, context.count, context.hostname);

  return (
    <div className="relative flex min-h-screen w-full items-center justify-center overflow-hidden bg-background px-6 py-16">
      {/* A single wash of the category colour. Enough to make gambling feel
          different from social media without turning the page into a warning. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 -top-64 h-[32rem] blur-[120px] opacity-25"
        style={{ background: `radial-gradient(circle, ${info.color} 0%, transparent 70%)` }}
      />

      <main className="relative w-full max-w-xl">
        <div className="rounded-2xl border border-border bg-surface/80 p-8 shadow-[var(--shadow-depth-lg)] backdrop-blur-xl sm:p-10">
          <div className="flex items-start gap-4">
            <div
              className="flex size-12 shrink-0 items-center justify-center rounded-xl"
              style={{ background: `${info.color}1f`, color: info.color }}
            >
              <Icon className="size-6" />
            </div>
            <div className="min-w-0">
              <p
                className="font-medium text-xs uppercase tracking-widest"
                style={{ color: info.color }}
              >
                {info.label}
              </p>
              <h1 className="mt-1 font-semibold text-2xl text-foreground leading-tight sm:text-3xl">
                Blocked by Focuser
              </h1>
            </div>
          </div>

          <p className="mt-6 text-base text-muted-foreground leading-relaxed">{message}</p>

          <dl className="mt-7 space-y-px overflow-hidden rounded-xl border border-border">
            <Row label="Site">
              <span className="truncate font-medium text-foreground">{context.hostname}</span>
            </Row>
            <Row label="Reason">
              <span className="text-muted-foreground">{REASON_LABEL[context.reason]}</span>
            </Row>
            {context.reason !== "domain" && context.reason !== "everything" && (
              <Row label="Matched">
                <code className="rounded bg-elevated px-1.5 py-0.5 font-mono text-[0.8rem] text-foreground">
                  {context.target}
                </code>
              </Row>
            )}
            <Row label="Attempts">
              <span className="text-muted-foreground">
                {context.count === 1 ? (
                  "First time today"
                ) : (
                  <>
                    <span className="font-medium text-foreground">{ordinal(context.count)}</span>{" "}
                    attempt
                  </>
                )}
              </span>
            </Row>
          </dl>

          <div className="mt-7 flex flex-wrap gap-3">
            <button
              type="button"
              onClick={() => {
                // `history.back()` would land on the blocked site again if the
                // user arrived here from it. A fresh tab page is the honest
                // "somewhere else" — and history.length is the only signal
                // available for whether there is anywhere to go back to.
                if (window.history.length > 1) window.history.back();
                else window.location.replace("about:blank");
              }}
              className="inline-flex items-center gap-2 rounded-lg bg-primary px-4 py-2.5 font-medium text-primary-foreground text-sm transition-colors hover:bg-primary-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
            >
              <ArrowLeft className="size-4" />
              Go back
            </button>
            <button
              type="button"
              onClick={() => window.location.replace("about:blank")}
              className="inline-flex items-center gap-2 rounded-lg border border-border-strong bg-elevated/60 px-4 py-2.5 font-medium text-foreground text-sm transition-colors hover:bg-hover"
            >
              <Globe className="size-4" />
              New tab
            </button>
          </div>
        </div>

        <p className="mt-5 text-center text-faint-foreground text-xs">
          Change what is blocked in the Focuser desktop app. Nothing here leaves your computer.
        </p>
      </main>
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-4 bg-elevated/40 px-4 py-3">
      <dt className="w-20 shrink-0 text-faint-foreground text-xs uppercase tracking-wide">
        {label}
      </dt>
      <dd className="min-w-0 flex-1 text-sm">{children}</dd>
    </div>
  );
}
