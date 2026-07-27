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
    <div className="flex h-screen overflow-hidden bg-deep">
      <nav
        aria-label="Main"
        className="flex w-60 shrink-0 flex-col gap-1 border-r border-border bg-base p-3"
      >
        <div className="px-3 py-4">
          <span className="font-semibold text-foreground text-lg tracking-tight">Focuser</span>
        </div>

        {NAV.map(({ to, label, icon: Icon, ...rest }) => (
          <NavLink
            key={to}
            to={to}
            end={"end" in rest ? rest.end : undefined}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors",
                "text-muted-foreground hover:bg-hover hover:text-foreground",
                isActive && "bg-primary-dim text-foreground",
              )
            }
          >
            <Icon aria-hidden className="size-4 shrink-0" />
            {label}
          </NavLink>
        ))}
      </nav>

      <main className="flex-1 overflow-y-auto bg-background">
        <Outlet />
      </main>
    </div>
  );
}
