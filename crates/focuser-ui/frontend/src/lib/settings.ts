/**
 * Typed access to the key/value settings table.
 *
 * Settings are stored as strings, so every read has to parse and every write
 * has to serialise. Doing that in one place keeps a stray `"True"` or `"60s"`
 * from ever reaching the database.
 */

import { useSetSetting, useSetting } from "./commands";

/** Keys the backend actually reads. Adding one here means something consumes it. */
export const SETTING_KEYS = {
  blockUnsupportedBrowsers: "block_unsupported_browsers",
  extensionGracePeriod: "extension_grace_period",
  language: "language",
} as const;

export const MAX_RETENTION_DAYS = 36_500;

export function useBooleanSetting(key: string, fallback: boolean) {
  const query = useSetting(key, String(fallback));
  const save = useSetSetting();

  return {
    value: query.data === undefined || query.data === null ? fallback : query.data === "true",
    isPending: query.isPending,
    isSaving: save.isPending,
    error: query.error ?? save.error,
    set: (next: boolean) => save.mutate({ key, value: String(next) }),
  };
}

export function useNumberSetting(key: string, fallback: number) {
  const query = useSetting(key, String(fallback));
  const save = useSetSetting();
  const parsed = Number(query.data);

  return {
    value: Number.isFinite(parsed) ? parsed : fallback,
    isPending: query.isPending,
    isSaving: save.isPending,
    error: query.error ?? save.error,
    set: (next: number) => save.mutate({ key, value: String(next) }),
  };
}
