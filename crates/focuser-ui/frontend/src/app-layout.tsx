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
import { TitleBar } from "@/components/title-bar";
import { LiveBadge } from "@/components/ui/badge";
import { usePomodoroStatus } from "@/lib/commands";
import { formatCountdown } from "@/lib/duration";
import { cn } from "@/lib/utils";

const NAV = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard, end: true },
  { to: "/block-lists", label: "Block Lists", icon: ListChecks },
  { to: "/websites", label: "Websites", icon: Globe },
  { to: "/apps", label: "Applications", icon: AppWindow },
  { to: "/schedule", label: "Schedule", icon: CalendarClock },
  { to: "/allowances", label: "Allowances", icon: Hourglass },
  { to: "/statistics", label: "Statistics", icon: BarChart3 },
  { to: "/settings", label: "Settings", icon: Settings },
] as const;

export function AppLayout() {
  return (
    <div className="flex h-screen flex-col overflow-hidden bg-deep">
      <TitleBar />

      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="app-canvas min-w-0 flex-1 overflow-y-auto bg-background">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

function Sidebar() {
  return (
    <nav
      aria-label="Main"
      className="flex w-56 shrink-0 flex-col border-border/60 border-r bg-deep/60 p-3"
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
                  ? "bg-primary-dim text-foreground"
                  : "text-muted-foreground hover:bg-hover/60 hover:text-foreground",
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
                    "-translate-y-1/2 absolute top-1/2 left-0 h-4 w-0.5 rounded-r-full bg-primary transition-opacity",
                    isActive ? "opacity-100" : "opacity-0",
                  )}
                />
                <Icon
                  aria-hidden
                  className={cn(
                    "size-4 shrink-0 transition-colors",
                    isActive ? "text-primary" : "text-faint-foreground group-hover:text-foreground",
                  )}
                />
                {label}
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
    <div className="mb-4 flex items-center gap-2.5 px-3 py-3">
      <span className="flex size-8 items-center justify-center rounded-lg bg-primary/15 ring-1 ring-primary/30 ring-inset">
        <Hourglass aria-hidden className="size-4 text-primary" />
      </span>
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

  const phase = status.data.current_phase === "work" ? "Focus" : "Break";

  return (
    <div className="mt-auto rounded-lg border border-border bg-surface/80 p-3">
      <div className="flex items-center justify-between gap-2">
        <LiveBadge tone={status.data.current_phase === "work" ? "primary" : "success"}>
          {status.data.paused ? "Paused" : phase}
        </LiveBadge>
        <span className="font-semibold text-foreground text-sm tabular-nums">
          {formatCountdown(status.data.remaining_secs)}
        </span>
      </div>
      <p className="mt-1.5 truncate text-faint-foreground text-xs">{status.data.block_list_name}</p>
    </div>
  );
}
