/**
 * "Launch at login" lives in the Tauri autostart plugin, not the command core —
 * it edits an OS-level registration that only the desktop app owns. In the
 * browser harness there is nothing to register, so the query is disabled and the
 * page shows the control as unavailable rather than pretending it works.
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
    queryFn: () => invoke<boolean>("plugin:autostart|is_enabled"),
    enabled: supported,
    refetchInterval: false,
  });

  const set = useMutation({
    mutationFn: (next: boolean) =>
      invoke(next ? "plugin:autostart|enable" : "plugin:autostart|disable"),
    onSuccess: () => qc.invalidateQueries({ queryKey }),
  });

  return {
    supported,
    value: enabled.data ?? false,
    isPending: supported && enabled.isPending,
    isSaving: set.isPending,
    error: (enabled.error ?? set.error) as Error | null,
    set: (next: boolean) => set.mutate(next),
  };
}
