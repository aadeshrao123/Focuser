import { useState } from "react";
import { Button } from "@/components/ui/button";
import { checkForUpdate, installUpdate, isTauri } from "@/lib/native";
import { m } from "@/paraglide/messages.js";

type State =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "current" }
  | { status: "available"; version?: string }
  | { status: "installing" }
  | { status: "failed"; message: string };

/** Manual update check. Only the desktop app has an updater to talk to. */
export function UpdateCheck() {
  const [state, setState] = useState<State>({ status: "idle" });

  if (!isTauri()) {
    return <span className="text-faint-foreground text-sm">{m.update_desktop_only()}</span>;
  }

  async function check() {
    setState({ status: "checking" });
    try {
      const result = await checkForUpdate();
      setState(
        result.available ? { status: "available", version: result.version } : { status: "current" },
      );
    } catch (e) {
      setState({ status: "failed", message: String(e) });
    }
  }

  async function install() {
    setState({ status: "installing" });
    try {
      await installUpdate();
      // The app restarts into the new version, so there is no success state.
    } catch (e) {
      setState({ status: "failed", message: String(e) });
    }
  }

  return (
    <div className="flex items-center gap-3">
      {state.status === "current" && (
        <span className="text-muted-foreground text-sm">{m.update_up_to_date()}</span>
      )}
      {state.status === "available" && (
        <span className="text-success text-sm">
          {state.version ? `Version ${state.version} available` : "Update available"}
        </span>
      )}
      {state.status === "failed" && (
        <span role="alert" className="max-w-64 truncate text-destructive text-sm">
          {state.message}
        </span>
      )}

      {state.status === "available" ? (
        <Button size="sm" onClick={install}>
          {m.update_install_restart()}
        </Button>
      ) : (
        <Button
          variant="outline"
          size="sm"
          onClick={check}
          disabled={state.status === "checking" || state.status === "installing"}
        >
          {state.status === "checking" ? m.update_checking() : m.update_check()}
        </Button>
      )}
    </div>
  );
}
