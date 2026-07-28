import {
  ArrowUpRight,
  Ban,
  Bug,
  Code,
  Heart,
  Laptop,
  Lock,
  Shield,
  Sparkles,
  Star,
  Timer,
} from "lucide-react";
import type { ComponentType } from "react";
import { showApp } from "@/lib/api";

const REPO = "https://github.com/aadeshrao123/Focuser";
/** A page that renders nothing is worse than one with no version on it. */
function extensionVersion(): string {
  try {
    return browser.runtime.getManifest().version;
  } catch {
    return "";
  }
}

const VERSION = extensionVersion();

/** Written down here rather than fetched, so the page works with no app running. */
const WHATS_NEW = [
  {
    title: "Rebuilt from scratch",
    body: "The extension was rewritten. It is smaller, faster to start, and one codebase now covers Chrome, Edge, Brave and Firefox.",
  },
  {
    title: "A block page worth reading",
    body: "It names the site, says which rule caught it, and counts how many times you have tried today. The message changes as that count climbs.",
  },
  {
    title: "The popup tells the truth",
    body: "It now shows which of your lists a site is actually in. Unblocking removes it from all of them instead of guessing at one.",
  },
  {
    title: "No flash of the site",
    body: "Blocking now takes effect before the page paints, so you never catch a glimpse of the thing you were avoiding.",
  },
];

const POINTS: Array<{ icon: ComponentType<{ className?: string }>; title: string; body: string }> = [
  {
    icon: Ban,
    title: "More than domains",
    body: "Match a whole site, a word in the address, a pattern, or one section of a site.",
  },
  {
    icon: Timer,
    title: "Only when you meant it",
    body: "Lists can follow a weekly schedule or run for one focus session.",
  },
  {
    icon: Lock,
    title: "Stays on your machine",
    body: "It talks to the Focuser app on your own computer and to nothing else.",
  },
];

const RULE_TYPES = [
  { kind: "Site", example: "reddit.com", covers: "The site and everything under it" },
  { kind: "Word", example: "casino", covers: "Any address containing that word" },
  { kind: "Pattern", example: "*.reddit.com", covers: "Anything matching the pattern" },
  { kind: "Section", example: "reddit.com/r/all", covers: "Just that part of a site" },
];

/** lucide dropped brand marks, so the GitHub logo is inline. */
function GitHubMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" aria-hidden className={className} fill="currentColor">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
    </svg>
  );
}

export function Welcome() {
  const updated = new URLSearchParams(window.location.search).get("reason") === "update";

  return (
    <div className="relative min-h-screen w-full overflow-hidden bg-background">
      <Ambient />

      <main className="relative mx-auto w-full max-w-3xl px-6 pt-20 pb-16">
        <header className="flex flex-col items-center text-center">
          <div className="relative">
            <div
              aria-hidden
              className="-inset-6 absolute rounded-full bg-primary/25 blur-3xl"
            />
            <img
              src="/icons/icon128.png"
              alt=""
              width={80}
              height={80}
              className="relative rounded-[1.35rem] shadow-[var(--shadow-depth-lg)]"
            />
          </div>

          {VERSION && (
            <p className="mt-6 inline-flex items-center gap-1.5 rounded-full border border-border-strong bg-elevated/70 px-3 py-1 font-medium text-[0.7rem] text-muted-foreground uppercase tracking-[0.16em] backdrop-blur">
              <Sparkles className="size-3 text-primary" />
              Version {VERSION}
            </p>
          )}

          <h1 className="mt-5 text-balance font-semibold text-4xl text-foreground leading-[1.1] sm:text-5xl">
            {updated ? "Focuser just got better" : "Thanks for installing Focuser"}
          </h1>
          <p className="mt-4 max-w-xl text-pretty text-base text-muted-foreground leading-relaxed sm:text-lg">
            {updated
              ? "Here is what changed in this version, and a quick reminder of how the pieces fit together."
              : "Focuser keeps distracting sites out of your way while you work. Here is everything you need to get going."}
          </p>
        </header>

        <Card className="mt-12">
          <SectionTitle icon={Laptop}>One thing to set up</SectionTitle>
          <p className="mt-3 text-muted-foreground leading-relaxed">
            The extension does not hold your block lists. It asks the Focuser desktop app for them,
            which is what lets one set of rules cover your browser and your applications at the same
            time. Keep the app running and the extension looks after the browser side by itself.
          </p>
          <p className="mt-3 text-muted-foreground leading-relaxed">
            If the app is closed, the toolbar icon shows a red mark and nothing gets blocked. That is
            the extension being honest with you rather than pretending to work.
          </p>

          <div className="mt-6 flex flex-wrap gap-3">
            <PrimaryButton onClick={() => void showApp()}>
              <Shield className="size-4" />
              Open the Focuser app
            </PrimaryButton>
            <SecondaryLink href={REPO}>
              <GitHubMark className="size-4" />
              Get the desktop app
            </SecondaryLink>
          </div>
        </Card>

        <section className="mt-14">
          <h2 className="font-semibold text-2xl text-foreground">{VERSION ? `What is new in ${VERSION}` : "What is new"}</h2>
          <ol className="mt-6 space-y-4">
            {WHATS_NEW.map((item, i) => (
              <li
                key={item.title}
                className="flex gap-4 rounded-2xl border border-border bg-surface/40 p-4 transition-colors hover:border-border-strong hover:bg-surface/70"
              >
                <span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-primary/15 font-semibold text-primary text-xs tabular-nums">
                  {i + 1}
                </span>
                <div className="min-w-0">
                  <h3 className="font-medium text-foreground">{item.title}</h3>
                  <p className="mt-1 text-muted-foreground text-sm leading-relaxed">{item.body}</p>
                </div>
              </li>
            ))}
          </ol>
        </section>

        <section className="mt-14 grid gap-4 sm:grid-cols-3">
          {POINTS.map(({ icon: Icon, title, body }) => (
            <div
              key={title}
              className="rounded-2xl border border-border bg-surface/50 p-5 backdrop-blur-sm transition-colors hover:border-border-strong"
            >
              <span className="flex size-9 items-center justify-center rounded-xl bg-primary/15 text-primary ring-1 ring-primary/20 ring-inset">
                <Icon className="size-4" />
              </span>
              <h3 className="mt-4 font-medium text-foreground">{title}</h3>
              <p className="mt-1.5 text-muted-foreground text-sm leading-relaxed">{body}</p>
            </div>
          ))}
        </section>

        <Card className="mt-14 overflow-hidden p-0">
          <div className="p-6 sm:p-7">
            <SectionTitle icon={Code}>The four ways to block something</SectionTitle>
            <p className="mt-3 text-muted-foreground leading-relaxed">
              Add these in the desktop app under Websites. The extension enforces all four.
            </p>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-left text-sm">
              <caption className="sr-only">Rule types and what each one covers</caption>
              <thead>
                <tr className="border-border border-y bg-elevated/40 text-faint-foreground text-xs">
                  <th scope="col" className="px-6 py-2.5 font-medium uppercase tracking-wide">
                    Kind
                  </th>
                  <th scope="col" className="px-6 py-2.5 font-medium uppercase tracking-wide">
                    Example
                  </th>
                  <th scope="col" className="px-6 py-2.5 font-medium uppercase tracking-wide">
                    What it covers
                  </th>
                </tr>
              </thead>
              <tbody>
                {RULE_TYPES.map((row) => (
                  <tr key={row.kind} className="border-border/60 border-b last:border-0">
                    <td className="px-6 py-3 font-medium text-foreground">{row.kind}</td>
                    <td className="px-6 py-3">
                      <code className="rounded-md bg-elevated px-2 py-1 font-mono text-[0.78rem] text-primary">
                        {row.example}
                      </code>
                    </td>
                    <td className="px-6 py-3 text-muted-foreground">{row.covers}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>

        <Card className="mt-14">
          <SectionTitle icon={Heart}>Free, and open source</SectionTitle>
          <p className="mt-3 text-muted-foreground leading-relaxed">
            Focuser is completely open source. Every line of the app and this extension is on
            GitHub, so you can read exactly what it does, check for yourself that nothing is sent
            anywhere, build it from source, or change it to suit how you work. There is nothing to
            pay for and no account to make.
          </p>
          <p className="mt-3 text-muted-foreground leading-relaxed">
            If you want to help, the repository is the place. Pull requests are welcome, and so are
            bug reports and ideas. If Focuser just saves you some time, a star is a kind way to say
            so.
          </p>

          <div className="mt-6 flex flex-wrap gap-3">
            <SecondaryLink href={REPO}>
              <Star className="size-4 text-warning" />
              Star the project
            </SecondaryLink>
            <SecondaryLink href={`${REPO}/issues`}>
              <Bug className="size-4" />
              Report a bug
            </SecondaryLink>
            <SecondaryLink href={`${REPO}/pulls`}>
              <Code className="size-4" />
              Contribute code
            </SecondaryLink>
          </div>
        </Card>

        <footer className="mt-16 text-center">
          <div
            aria-hidden
            className="mx-auto h-px w-40"
            style={{
              background:
                "linear-gradient(90deg, transparent, rgb(255 255 255 / 0.14), transparent)",
            }}
          />
          <p className="mt-6 text-muted-foreground text-sm">
            Thank you for using Focuser. Good luck with whatever you are trying to get done.
          </p>
          <p className="mt-1.5 text-faint-foreground text-xs">
            Made for people who would rather be doing something else.
          </p>
        </footer>
      </main>
    </div>
  );
}

function Ambient() {
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0">
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(56rem 30rem at 50% -12%, rgb(139 92 246 / 0.30), transparent 66%)",
        }}
      />
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(44rem 26rem at 12% 40%, rgb(34 211 238 / 0.08), transparent 62%)",
        }}
      />
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(40rem 24rem at 88% 78%, rgb(244 114 182 / 0.07), transparent 62%)",
        }}
      />
    </div>
  );
}

function Card({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return (
    <section
      className={`rounded-[1.25rem] border border-border-strong bg-surface/70 p-6 shadow-[var(--shadow-depth-md)] backdrop-blur-xl sm:p-7 ${className}`}
    >
      {children}
    </section>
  );
}

function SectionTitle({
  icon: Icon,
  children,
}: {
  icon: ComponentType<{ className?: string }>;
  children: React.ReactNode;
}) {
  return (
    <h2 className="flex items-center gap-2.5 font-semibold text-foreground text-xl">
      <span className="flex size-8 items-center justify-center rounded-lg bg-primary/15 text-primary">
        <Icon className="size-4" />
      </span>
      {children}
    </h2>
  );
}

function PrimaryButton({
  children,
  onClick,
}: {
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2.5 font-medium text-primary-foreground text-sm shadow-[0_6px_20px_-8px_var(--color-primary)] transition-[transform,background-color,box-shadow] duration-200 hover:-translate-y-0.5 hover:bg-primary-hover hover:shadow-[0_12px_28px_-10px_var(--color-primary)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary active:translate-y-0"
    >
      {children}
    </button>
  );
}

function SecondaryLink({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className="inline-flex items-center gap-2 rounded-xl border border-border-strong bg-elevated/70 px-4 py-2.5 font-medium text-foreground text-sm transition-colors hover:bg-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
    >
      {children}
      <ArrowUpRight className="size-3.5 text-faint-foreground" />
    </a>
  );
}
