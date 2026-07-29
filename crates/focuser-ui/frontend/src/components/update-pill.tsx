import { ArrowUpCircle } from "lucide-react";
import { Link } from "react-router-dom";
import { useUpdate } from "@/lib/updates";
import { m } from "@/paraglide/messages.js";

/**
 * A new version, mentioned in the sidebar rather than thrown at you in a
 * dialog. Renders nothing when there is no update or the check failed.
 */
export function UpdatePill() {
  const update = useUpdate();
  if (!update.data?.available) return null;

  return (
    <Link
      to="/settings?highlight=updates"
      className="glass-strong group flex items-center gap-2.5 rounded-lg border border-primary/30 p-2.5 transition-colors hover:border-primary/50 hover:bg-primary/10"
    >
      <ArrowUpCircle aria-hidden className="size-4 shrink-0 text-primary" />
      <div className="min-w-0">
        <p className="truncate font-medium text-foreground text-xs">{m.update_available()}</p>
        {update.data.version && (
          <p className="truncate text-faint-foreground text-[0.7rem]">
            {m.update_version_ready({ version: update.data.version })}
          </p>
        )}
      </div>
    </Link>
  );
}
