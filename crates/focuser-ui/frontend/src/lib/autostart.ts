/**
 * "Launch at login" is an OS-level registration only the desktop app owns, so
 * it lives outside the command core. In the browser harness there is nothing to
 * register and the page shows the control as unavailable.
 *
 * Goes through our own commands rather than `plugin:autostart|*` because on
 * Windows the installer also creates a scheduled task, and the plugin knows
 * nothing about it.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./transport";

const queryKey = ["autostart"] as const;

export function useAutostart() {
  const qc = useQueryClient();
  const supported = isTauri();

  const enabled = useQuery({
    queryKey,
    queryFn: () => invoke<boolean>("is_autostart_enabled"),
    enabled: supported,
    refetchInterval: false,
  });

  const set = useMutation({
    mutationFn: (next: boolean) => invoke("set_autostart", { enabled: next }),
    // Settled, not success: the choice is saved even when the scheduled task
    // refuses, so the toggle has to move either way.
    onSettled: () => qc.invalidateQueries({ queryKey }),
  });

  // Windows only, and only for the installer's logon task, which needs admin to
  // change. The setting is already saved and startup finishes the job, so this
  // is a note about timing rather than a failure.
  const needsAdmin = String(set.error ?? "").includes("needs-admin");

  return {
    supported,
    value: enabled.data ?? false,
    isPending: supported && enabled.isPending,
    isSaving: set.isPending,
    needsAdmin,
    error: (enabled.error ?? (needsAdmin ? null : set.error)) as Error | null,
    set: (next: boolean) => set.mutate(next),
  };
}
