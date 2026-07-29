import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { checkForUpdate, installUpdate, isTauri } from "./native";

export const updateQueryKey = ["update"] as const;

const SIX_HOURS = 6 * 60 * 60 * 1000;

/** One query for the whole app, so the sidebar badge and Settings agree. */
export function useUpdate() {
  return useQuery({
    queryKey: updateQueryKey,
    queryFn: checkForUpdate,
    enabled: isTauri(),
    staleTime: SIX_HOURS,
    refetchInterval: SIX_HOURS,
    refetchOnWindowFocus: false,
    // No network or an unsigned build both land here. Neither is worth
    // retrying, and neither is worth showing in the sidebar.
    retry: false,
  });
}

export function useInstallUpdate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: installUpdate,
    // Only reached if the installer declined to restart.
    onSettled: () => qc.invalidateQueries({ queryKey: updateQueryKey }),
  });
}
