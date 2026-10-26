import { ShieldAlert } from "lucide-react";
import { useAuthoritySession, useBlockingHealth } from "@/lib/commands";
import { m } from "@/paraglide/messages.js";

export function BlockingHealthBanner() {
  const health = useBlockingHealth();
  const session = useAuthoritySession();
  const disconnected = health.isError || session.isError;
  const degraded = session.data === "missing";

  if (!disconnected && !degraded && !health.data?.active_lists) return null;
  if (
    !disconnected &&
    !degraded &&
    (health.data?.extension_connected || health.data?.hosts_writable)
  )
    return null;

  return (
    <div
      role="alert"
      className="flex items-start gap-3 border-warning/30 border-b bg-warning/10 px-6 py-3"
    >
      <ShieldAlert aria-hidden className="mt-0.5 size-4 shrink-0 text-warning" />
      <div className="min-w-0 text-sm">
        <p className="font-medium text-foreground">
          {disconnected
            ? m.authority_disconnected_title()
            : degraded
              ? m.authority_degraded_title()
              : m.health_title()}
        </p>
        <p className="mt-0.5 text-muted-foreground">
          {disconnected
            ? m.authority_disconnected_body()
            : degraded
              ? m.authority_degraded_body()
              : m.health_body()}
        </p>
      </div>
    </div>
  );
}
