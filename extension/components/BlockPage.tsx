import { i18n } from "#i18n";
import {
  ArrowLeft,
  Clock,
  Dice5,
  ExternalLink,
  Gamepad2,
  Heart,
  MessageCircle,
  Newspaper,
  PlayCircle,
  Shield,
  ShieldAlert,
  ShoppingCart,
  Target,
  X,
} from "lucide-react";
import { type ComponentType, Fragment, type ReactNode, useCallback, useEffect } from "react";
import { categoryInfo, messageFor, resolveCategory } from "@/lib/categories";
import { type BlockContext, send } from "@/lib/messages";

/**
 * Keyed on the lucide icon name the category data already carries, not on the
 * category id. Adding a category that reuses an existing glyph then needs no
 * change here, and anything unrecognised still renders.
 */
export const ICONS: Record<string, ComponentType<{ className?: string }>> = {
  shield: Shield,
  "shield-alert": ShieldAlert,
  "message-circle": MessageCircle,
  "play-circle": PlayCircle,
  heart: Heart,
  "gamepad-2": Gamepad2,
  newspaper: Newspaper,
  "shopping-cart": ShoppingCart,
  "dice-5": Dice5,
};

// Functions, not strings: `i18n.t` reads the locale when it runs, and a module
// constant would freeze whatever was active when this file was imported.
const REASON_LABEL: Record<BlockContext["reason"], () => string> = {
  domain: () => i18n.t("block.reason.domain"),
  keyword: () => i18n.t("block.reason.keyword"),
  "url-path": () => i18n.t("block.reason.urlPath"),
  wildcard: () => i18n.t("block.reason.wildcard"),
  everything: () => i18n.t("block.reason.everything"),
};

/** Past this, a hostname at full display size wraps to three lines. */
const LONG_HOSTNAME = 34;

/**
 * How many times today, as a sentence.
 *
 * Was an English ordinal — "1st", "2nd", "3rd". Those do not translate: Spanish
 * writes "1.º" and plenty of languages do not form ordinals that way at all, so
 * the message carries the whole phrase and picks its own plural form.
 */
export function attemptLabel(n: number): string {
  return i18n.t("block.attempt", n);
}

export function BlockPage({ context }: { context: BlockContext }) {
  const category = resolveCategory(context.category);
  const info = categoryInfo(category);
  const Icon = ICONS[info.icon] ?? Shield;
  const message = messageFor(category, context.count, context.hostname);
  const accent = info.color;
  const long = context.hostname.length > LONG_HOSTNAME;

  const closeTab = useCallback(async () => {
    // A content script cannot close its own tab — `window.close()` only works
    // on tabs that script opened — so the background does it.
    const reply = await send({ type: "close-tab" });
    if (!reply?.ok) window.location.replace("about:blank");
  }, []);

  const goBack = useCallback(() => {
    // Back lands on wherever they came from, because the block page replaced
    // the blocked URL in place rather than pushing a new entry.
    if (window.history.length > 1) window.history.back();
    else void closeTab();
  }, [closeTab]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") goBack();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [goBack]);

  return (
    <div className="relative min-h-screen w-full overflow-hidden bg-background">
      <Ambient accent={accent} />

      <div className="relative mx-auto flex min-h-screen w-full max-w-[40rem] flex-col justify-center px-5 py-12 sm:px-6">
        <div
          className="mb-7 flex animate-rise items-center gap-2 self-center"
          style={{ animationDelay: "40ms" }}
        >
          <Shield className="size-4 text-primary" />
          <span className="font-semibold text-[0.72rem] text-muted-foreground uppercase tracking-[0.22em]">
            Focuser
          </span>
        </div>

        <div className="relative animate-rise" style={{ animationDelay: "120ms" }}>
          {/* The card sits in its own pool of light rather than on flat black.
              Without this it reads as a rectangle cut out of the page. */}
          <div
            aria-hidden
            className="-inset-8 absolute rounded-[2.5rem] blur-3xl"
            style={{
              background: `radial-gradient(65% 60% at 50% 0%, ${accent}2e, transparent 72%)`,
            }}
          />

          <article className="relative overflow-hidden rounded-[1.25rem] border border-border-strong bg-surface/80 p-6 shadow-[var(--shadow-depth-lg)] backdrop-blur-2xl sm:p-9">
            {/* Accent hairline along the top edge, fading at both ends. */}
            <div
              aria-hidden
              className="absolute inset-x-0 top-0 h-px"
              style={{
                background: `linear-gradient(90deg, transparent, ${accent}99 28%, ${accent}99 72%, transparent)`,
              }}
            />
            <header className="relative flex items-start gap-4">
              <div className="relative shrink-0">
                <div
                  aria-hidden
                  className="-inset-2.5 absolute animate-halo rounded-full blur-lg"
                  style={{ background: `${accent}3d` }}
                />
                <div
                  className="relative flex size-14 items-center justify-center rounded-2xl border"
                  style={{
                    backgroundColor: `${accent}24`,
                    borderColor: `${accent}5c`,
                    color: accent,
                  }}
                >
                  <Icon className="size-7" />
                </div>
              </div>

              <div className="min-w-0 flex-1">
                <p
                  className="flex items-center gap-1.5 font-semibold text-[0.7rem] uppercase tracking-[0.16em]"
                  style={{ color: accent }}
                >
                  <span
                    aria-hidden
                    className="size-1.5 rounded-full"
                    style={{ backgroundColor: accent }}
                  />
                  {i18n.t("block.blocked")}
                </p>
                <h1
                  className={`mt-1 font-semibold text-foreground leading-[1.15] [overflow-wrap:anywhere] ${
                    long ? "text-xl sm:text-2xl" : "text-[1.6rem] sm:text-3xl"
                  }`}
                >
                  <HostName value={context.hostname} />
                </h1>
                <p className="mt-1.5 text-muted-foreground text-sm">
                  {info.label} · {REASON_LABEL[context.reason]()}
                </p>
              </div>
            </header>

            {/* A hairline that fades at both ends — a full-width rule cuts the
                card in two, which is heavier than this needs to be. */}
            <div
              aria-hidden
              className="relative my-6 h-px"
              style={{
                background:
                  "linear-gradient(90deg, transparent, rgb(255 255 255 / 0.11) 18%, rgb(255 255 255 / 0.11) 82%, transparent)",
              }}
            />

            <p className="relative text-pretty text-[1.0625rem] text-foreground/90 leading-[1.65]">
              {message}
            </p>

            <div className="relative mt-6 flex flex-wrap items-center gap-2">
              <Chip icon={Clock}>
                {attemptLabel(context.count)}
              </Chip>
              {context.reason !== "domain" && context.reason !== "everything" && (
                <Chip icon={Target}>
                  <span className="font-mono text-[0.78rem]">{context.target}</span>
                </Chip>
              )}
            </div>

            <div className="relative mt-7 flex flex-col gap-2.5 sm:flex-row">
              <button
                type="button"
                onClick={goBack}
                className="inline-flex flex-1 items-center justify-center gap-2 rounded-xl bg-primary px-4 py-2.5 font-medium text-primary-foreground text-sm shadow-[0_6px_20px_-8px_var(--color-primary)] transition-[transform,background-color,box-shadow] duration-200 hover:-translate-y-0.5 hover:bg-primary-hover hover:shadow-[0_12px_28px_-10px_var(--color-primary)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary active:translate-y-0 active:bg-primary-active"
              >
                <ArrowLeft className="size-4" />
                {i18n.t("block.goBack")}
              </button>
              <button
                type="button"
                onClick={() => void closeTab()}
                className="inline-flex flex-1 items-center justify-center gap-2 rounded-xl border border-border-strong bg-elevated/70 px-4 py-2.5 font-medium text-foreground text-sm transition-colors duration-200 hover:bg-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
              >
                <X className="size-4" />
                {i18n.t("block.closeTab")}
              </button>
            </div>
          </article>
        </div>

        <footer
          className="mt-5 flex animate-rise flex-wrap items-center justify-center gap-x-3 gap-y-2 text-faint-foreground text-xs"
          style={{ animationDelay: "220ms" }}
        >
          <span className="inline-flex items-center gap-1.5">
            {i18n.t("block.pressEsc")}
            <kbd className="rounded border border-border-strong bg-elevated px-1.5 py-0.5 font-sans text-[0.68rem] text-muted-foreground">
              Esc
            </kbd>
            {i18n.t("block.toGoBack")}
          </span>
          <span aria-hidden className="text-border-strong">
            ·
          </span>
          <button
            type="button"
            onClick={() => void send({ type: "open-app" })}
            className="inline-flex items-center gap-1 rounded transition-colors hover:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
          >
            {i18n.t("block.openApp")}
            <ExternalLink className="size-3" />
          </button>
        </footer>

        <p className="mt-3 text-center text-[0.7rem] text-faint-foreground/70">
          {i18n.t("block.privacy")}
        </p>
      </div>
    </div>
  );
}

/**
 * A hostname has no spaces, so the browser breaks it mid-label and produces
 * things like `subdomai / n.example.com`. Offering a break opportunity after
 * every dot keeps the breaks on the separators, where they read as intended.
 */
function HostName({ value }: { value: string }) {
  const labels = value.split(".");
  return (
    <>
      {labels.map((label, i) => (
        // Index keys are safe here: the list is derived and never reordered.
        <Fragment key={`${i}-${label}`}>
          {i > 0 && (
            <>
              .<wbr />
            </>
          )}
          {label}
        </Fragment>
      ))}
    </>
  );
}

/** Layered washes rather than an image: no request to make, nothing for the
    blocked site's CSP to refuse. */
function Ambient({ accent }: { accent: string }) {
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0">
      <div
        className="absolute inset-0"
        style={{
          background: `radial-gradient(56rem 30rem at 50% -12%, ${accent}4d, transparent 66%)`,
        }}
      />
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(46rem 26rem at 50% 116%, rgb(139 92 246 / 0.22), transparent 68%)",
        }}
      />
      <div
        className="absolute inset-0"
        style={{
          background: "radial-gradient(128% 92% at 50% 0%, transparent 38%, rgb(6 6 10 / 0.82) 100%)",
        }}
      />
    </div>
  );
}

function Chip({ icon: Icon, children }: { icon: ComponentType<{ className?: string }>; children: ReactNode }) {
  return (
    <span className="inline-flex max-w-full items-center gap-1.5 rounded-lg border border-border bg-elevated/50 px-2.5 py-1.5 text-muted-foreground text-xs">
      <Icon className="size-3.5 shrink-0 text-faint-foreground" />
      <span className="truncate">{children}</span>
    </span>
  );
}
