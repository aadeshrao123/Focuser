import {
  AppWindow,
  BarChart3,
  CalendarClock,
  Globe,
  Hourglass,
  LayoutDashboard,
  ListChecks,
  Settings,
} from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";
import { BlockingHealthBanner } from "@/components/blocking-health-banner";
import { TitleBar } from "@/components/title-bar";
import { AppIcon } from "@/components/ui/app-icon";
import { LiveBadge } from "@/components/ui/badge";
import { usePomodoroStatus } from "@/lib/commands";
import { formatCountdown } from "@/lib/duration";
import { useApplySavedLanguage } from "@/lib/language";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages.js";

// `label` is a function, not a string: a message read at module scope would
// freeze the locale that was active when this file was imported.
const NAV = [
  { to: "/", label: m.nav_dashboard, icon: LayoutDashboard, end: true },
  { to: "/block-lists", label: m.nav_block_lists, icon: ListChecks },
  { to: "/websites", label: m.nav_websites, icon: Globe },
  { to: "/apps", label: m.nav_applications, icon: AppWindow },
  { to: "/schedule", label: m.nav_schedule, icon: CalendarClock },
  { to: "/allowances", label: m.nav_allowances, icon: Hourglass },
  { to: "/statistics", label: m.nav_statistics, icon: BarChart3 },
  { to: "/settings", label: m.nav_settings, icon: Settings },
] as const;

export function AppLayout() {
  // Wraps every route, so this is the one place the saved language reaches the
  // whole app rather than only the page that happens to read it.
  useApplySavedLanguage();

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-deep">
      <TitleBar />

      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="app-canvas min-w-0 flex-1 overflow-y-auto bg-background">
          <BlockingHealthBanner />
          <Outlet />
        </main>
      </div>
    </div>
  );
}

function Sidebar() {
  return (
    <nav
      aria-label={m.nav_landmark()}
      className="glass flex w-56 shrink-0 flex-col border-border/60 border-r p-3"
    >
      <Brand />

      <div className="flex flex-col gap-0.5">
        {NAV.map(({ to, label, icon: Icon, ...rest }) => (
          <NavLink
            key={to}
            to={to}
            end={"end" in rest ? rest.end : undefined}
            className={({ isActive }) =>
              cn(
                "group relative flex items-center gap-3 rounded-lg px-3 py-2 font-medium text-sm",
                "transition-colors duration-150",
                isActive
                  ? "bg-primary/12 text-foreground"
                  : // A hairline tint on hover. The previous fill was a solid
                    // block the width of the sidebar, which read as a selection.
                    "text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground",
              )
            }
          >
            {({ isActive }) => (
              <>
                {/* A bar rather than a whole-row fill, so the current page is
                    findable at a glance without a second colour block. */}
                <span
                  aria-hidden
                  className={cn(
                    "-translate-y-1/2 absolute top-1/2 left-0 w-[3px] rounded-r-full bg-primary",
                    "transition-all duration-200",
                    isActive ? "h-5 opacity-100" : "h-0 opacity-0",
                  )}
                />
                <Icon
                  aria-hidden
                  className={cn(
                    "size-4 shrink-0 transition-colors",
                    isActive ? "text-primary" : "text-faint-foreground group-hover:text-foreground",
                  )}
                />
                {label()}
              </>
            )}
          </NavLink>
        ))}
      </div>

      <SessionPill />
    </nav>
  );
}

function Brand() {
  return (
    <div className="mb-5 flex items-center gap-2.5 px-2 py-3">
      <AppIcon className="size-8 rounded-lg" />
      <span className="font-semibold text-base text-foreground tracking-tight">Focuser</span>
    </div>
  );
}

/**
 * A running focus session, pinned to the bottom of the sidebar.
 *
 * The countdown is the one thing worth seeing from every page — otherwise you
 * have to keep returning to the Dashboard to check it.
 */
function SessionPill() {
  const status = usePomodoroStatus();
  if (!status.data) return null;

  const phase = status.data.current_phase === "work" ? m.session_focus() : m.session_break();

  return (
    <div className="glass-strong mt-auto rounded-lg border border-primary/25 p-3">
      <div className="flex items-center justify-between gap-2">
        <LiveBadge tone={status.data.current_phase === "work" ? "primary" : "success"}>
          {status.data.paused ? m.session_paused() : phase}
        </LiveBadge>
        <span className="font-semibold text-foreground text-sm tabular-nums">
          {formatCountdown(status.data.remaining_secs)}
        </span>
      </div>
      <p className="mt-1.5 truncate text-faint-foreground text-xs">{status.data.block_list_name}</p>
    </div>
  );
}
