import { Puzzle, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { BrowserStatusList } from "@/components/browser-status";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useBlockingHealth } from "@/lib/commands";
import { hostPlatform } from "@/lib/native";
import { m } from "@/paraglide/messages.js";

/**
 * Warns when block lists are on but nothing can block websites.
 *
 * Focuser blocks sites two ways: the browser extension, or the OS hosts file.
 * The hosts file needs administrator or root, and the write failure is
 * invisible — rules look armed and sites keep loading. Programs are closed
 * either way, so this is about websites only.
 *
 * Deliberately silent unless *both* routes are unavailable and there is
 * something to block. A warning that shows when everything is fine gets
 * ignored when it isn't.
 */
export function BlockingHealthBanner() {
  const health = useBlockingHealth();
  const [installing, setInstalling] = useState(false);

  if (!health.data?.active_lists) return null;
  if (health.data.extension_connected || health.data.hosts_writable) return null;

  // Only Windows has a way to run the app with the rights for the hosts file.
  // Elsewhere that advice would mean running a desktop app as root.
  const body = hostPlatform() === "windows" ? m.health_body() : m.health_body_extension_only();

  return (
    <>
      <div
        role="alert"
        className="flex items-center gap-3 border-warning/30 border-b bg-warning/10 px-6 py-3"
      >
        <ShieldAlert aria-hidden className="mt-0.5 size-4 shrink-0 self-start text-warning" />
        <div className="min-w-0 flex-1 text-sm">
          <p className="font-medium text-foreground">{m.health_title()}</p>
          <p className="mt-0.5 text-muted-foreground">{body}</p>
        </div>
        <Button variant="outline" size="sm" icon={<Puzzle />} onClick={() => setInstalling(true)}>
          {m.health_get_extension()}
        </Button>
      </div>
      <Dialog
        open={installing}
        onClose={() => setInstalling(false)}
        title={m.settings_section_extension()}
        icon={<Puzzle aria-hidden className="text-primary" />}
      >
        <div className="mt-2">
          <BrowserStatusList />
        </div>
      </Dialog>
    </>
  );
}
