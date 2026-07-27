import { ShieldAlert } from "lucide-react";
import { useBlockingHealth } from "@/lib/commands";

/**
 * Warns when block lists are on but nothing can actually enforce them.
 *
 * Focuser blocks two ways: the browser extension, or the OS hosts file. The
 * hosts file needs administrator or root, and the write failure is invisible —
 * rules look armed and sites keep loading. Without this, the app quietly lies.
 *
 * Deliberately silent unless *both* routes are unavailable and there is
 * something to block. A warning that shows when everything is fine gets
 * ignored when it isn't.
 */
export function BlockingHealthBanner() {
  const health = useBlockingHealth();

  if (!health.data?.active_lists) return null;
  if (health.data.extension_connected || health.data.hosts_writable) return null;

  return (
    <div
      role="alert"
      className="flex items-start gap-3 border-warning/30 border-b bg-warning/10 px-6 py-3"
    >
      <ShieldAlert aria-hidden className="mt-0.5 size-4 shrink-0 text-warning" />
      <div className="min-w-0 text-sm">
        <p className="font-medium text-foreground">Blocking is not in force</p>
        <p className="mt-0.5 text-muted-foreground">
          Focuser cannot write the system hosts file, and no browser extension is connected — so
          your rules are saved but nothing is being blocked. Install the browser extension, or
          restart Focuser as administrator.
        </p>
      </div>
    </div>
  );
}
