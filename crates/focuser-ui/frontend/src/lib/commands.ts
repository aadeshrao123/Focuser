/**
 * Typed command helpers and their TanStack Query bindings.
 *
 * `Command`, `CommandResult`, and every domain type are **generated** from Rust
 * by tauri-specta into `src/bindings.ts`. Nothing here restates a backend type —
 * change a Rust struct or add a `Command` variant and this file stops compiling
 * until it is updated, which is the whole point.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type {
  Allowance,
  AllowanceMatch,
  AllowanceStatus,
  AppMatchType,
  BlockedEvent,
  BlockList,
  BrowserStatus,
  Command,
  CommandResult,
  ExceptionType,
  PomodoroConfig,
  PomodoroStatus,
  ProtectionInfo,
  TimeSlot,
  UsageStat,
  WebsiteMatchType,
  WebsiteRuleKind,
} from "@/bindings";
import { send } from "./transport";

export type {
  Allowance,
  AllowanceMatch,
  AllowanceStatus,
  AppMatchType,
  BlockedEvent,
  BlockList,
  BrowserStatus,
  Command,
  CommandResult,
  ExceptionType,
  PomodoroConfig,
  PomodoroStatus,
  ProtectionInfo,
  TimeSlot,
  UsageStat,
  WebsiteMatchType,
  WebsiteRuleKind,
};

/** Narrow a result, failing loudly if the backend sent a different variant. */
function expect<K extends CommandResult["kind"]>(
  result: CommandResult,
  kind: K,
): Extract<CommandResult, { kind: K }> {
  if (result.kind !== kind) {
    throw new Error(`expected a "${kind}" result, backend returned "${result.kind}"`);
  }
  return result as Extract<CommandResult, { kind: K }>;
}

const run = (command: Command) => send<CommandResult>(command);

// ─── Query keys ─────────────────────────────────────────────────────

export const queryKeys = {
  blockLists: ["block-lists"] as const,
  protection: ["protection"] as const,
  allowances: ["allowances"] as const,
  pomodoro: ["pomodoro"] as const,
  stats: (from: string, to: string) => ["stats", from, to] as const,
  events: (from: string, to: string) => ["events", from, to] as const,
  retention: ["stats-retention"] as const,
  browsers: ["browsers"] as const,
  setting: (key: string) => ["setting", key] as const,
  appIcons: (targets: string[]) => ["app-icons", ...targets] as const,
};

/** Anything that changes blocking invalidates lists and protection together. */
function useBlockListMutation<TArgs>(build: (args: TArgs) => Command) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: TArgs) => run(build(args)),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.blockLists });
      qc.invalidateQueries({ queryKey: queryKeys.protection });
    },
  });
}

// ─── Block lists ────────────────────────────────────────────────────

export function useBlockLists() {
  return useQuery({
    queryKey: queryKeys.blockLists,
    queryFn: async () => expect(await run({ cmd: "list_block_lists" }), "block_lists").data,
  });
}

export function useCreateBlockList() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) =>
      expect(await run({ cmd: "create_block_list", args: { name } }), "block_list").data,
    onSuccess: () => qc.invalidateQueries({ queryKey: queryKeys.blockLists }),
  });
}

export const useToggleBlockList = () =>
  useBlockListMutation<{ id: string; enabled: boolean }>((args) => ({
    cmd: "toggle_block_list",
    args,
  }));

export const useDeleteBlockList = () =>
  useBlockListMutation<string>((id) => ({ cmd: "delete_block_list", args: { id } }));

export const useUpdateBlockList = () =>
  useBlockListMutation<BlockList>((list) => ({ cmd: "update_block_list", args: { list } }));

// ─── Website rules ──────────────────────────────────────────────────

export const useAddWebsiteRule = () =>
  useBlockListMutation<{ listId: string; rule: WebsiteMatchType }>(({ listId, rule }) => ({
    cmd: "add_website_rule",
    args: { list_id: listId, rule },
  }));

export const useRemoveWebsiteRule = () =>
  useBlockListMutation<{ listId: string; ruleId: string }>(({ listId, ruleId }) => ({
    cmd: "remove_website_rule",
    args: { list_id: listId, rule_id: ruleId },
  }));

export const useBulkImportWebsites = () =>
  useBlockListMutation<{ listId: string; values: string[]; kind: WebsiteRuleKind }>(
    ({ listId, values, kind }) => ({
      cmd: "bulk_import_websites",
      args: { list_id: listId, values, kind },
    }),
  );

export const useClearAllWebsites = () =>
  useBlockListMutation<void>(() => ({ cmd: "clear_all_websites" }));

// ─── Application rules ──────────────────────────────────────────────

export const useAddAppRule = () =>
  useBlockListMutation<{ listId: string; rule: AppMatchType }>(({ listId, rule }) => ({
    cmd: "add_app_rule",
    args: { list_id: listId, rule },
  }));

export const useRemoveAppRule = () =>
  useBlockListMutation<{ listId: string; ruleId: string }>(({ listId, ruleId }) => ({
    cmd: "remove_app_rule",
    args: { list_id: listId, rule_id: ruleId },
  }));

export const useClearAllApps = () => useBlockListMutation<void>(() => ({ cmd: "clear_all_apps" }));

// ─── Exceptions ─────────────────────────────────────────────────────

export const useAddException = () =>
  useBlockListMutation<{ listId: string; exception: ExceptionType }>(({ listId, exception }) => ({
    cmd: "add_exception",
    args: { list_id: listId, exception },
  }));

export const useRemoveException = () =>
  useBlockListMutation<{ listId: string; exceptionId: string }>(({ listId, exceptionId }) => ({
    cmd: "remove_exception",
    args: { list_id: listId, exception_id: exceptionId },
  }));

// ─── Schedule ───────────────────────────────────────────────────────

export const useUpdateSchedule = () =>
  useBlockListMutation<{ listId: string; slots: TimeSlot[]; alwaysActive: boolean }>(
    ({ listId, slots, alwaysActive }) => ({
      cmd: "update_schedule",
      args: { list_id: listId, slots, always_active: alwaysActive },
    }),
  );

// ─── Statistics ─────────────────────────────────────────────────────

export function useStats(from: string, to: string) {
  return useQuery({
    queryKey: queryKeys.stats(from, to),
    queryFn: async () => expect(await run({ cmd: "get_stats", args: { from, to } }), "stats").data,
  });
}

export function useBlockedEvents(from: string, to: string) {
  return useQuery({
    queryKey: queryKeys.events(from, to),
    queryFn: async () =>
      expect(await run({ cmd: "get_blocked_events", args: { from, to } }), "blocked_events").data,
  });
}

export function useStatsRetention() {
  return useQuery({
    queryKey: queryKeys.retention,
    queryFn: async () => expect(await run({ cmd: "get_stats_retention" }), "count").data,
  });
}

export function useSetStatsRetention() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (days: number) => run({ cmd: "set_stats_retention", args: { days } }),
    onSuccess: () => qc.invalidateQueries({ queryKey: queryKeys.retention }),
  });
}

export function useClearStatistics() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => run({ cmd: "clear_statistics" }),
    // Keyed by date range, so invalidate by prefix.
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["stats"] });
      qc.invalidateQueries({ queryKey: ["events"] });
    },
  });
}

// ─── Whole configuration ────────────────────────────────────────────

/** The exported document as text. Where it is written is the caller's problem. */
export function useExportConfiguration() {
  return useMutation({
    mutationFn: async () => expect(await run({ cmd: "export_configuration" }), "text").data,
  });
}

export function useImportConfiguration() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (json: string) =>
      expect(await run({ cmd: "import_configuration", args: { json } }), "count").data,
    onSuccess: () => qc.invalidateQueries(),
  });
}

/** Wipes everything, so every cached query is stale afterwards. */
export function useDeleteAllData() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => run({ cmd: "delete_all_data" }),
    onSuccess: () => qc.invalidateQueries(),
  });
}

// ─── Diagnostics ────────────────────────────────────────────────────

export function useBrowserStatus() {
  return useQuery({
    queryKey: queryKeys.browsers,
    queryFn: async () => expect(await run({ cmd: "get_browser_status" }), "browser_status").data,
    // Browsers start and stop while the window is open, but not every 2s.
    refetchInterval: 10_000,
  });
}

/**
 * Real icons for application rules, keyed by the rule's value.
 *
 * An executable's icon only changes when the program is reinstalled, so this
 * never goes stale on its own — otherwise the Applications page would re-read
 * every icon off disk on each poll.
 */
export function useAppIcons(targets: string[]) {
  const key = [...targets].sort();

  return useQuery({
    queryKey: queryKeys.appIcons(key),
    enabled: key.length > 0,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
    queryFn: async () => {
      const icons = expect(
        await run({ cmd: "get_app_icons", args: { targets: key } }),
        "app_icons",
      ).data;
      return new Map(icons.map((i) => [i.target, i.data_uri]));
    },
  });
}

export function useAppVersion() {
  return useQuery({
    queryKey: ["app-version"] as const,
    queryFn: async () => expect(await run({ cmd: "app_version" }), "text").data,
    staleTime: Number.POSITIVE_INFINITY,
    refetchInterval: false,
  });
}

// ─── Protection ─────────────────────────────────────────────────────

export function useProtectionStatus() {
  return useQuery({
    queryKey: queryKeys.protection,
    queryFn: async () =>
      expect(await run({ cmd: "get_protection_status" }), "protection_status").data,
  });
}

export const useEnableProtection = () =>
  useBlockListMutation<{
    listId: string;
    minutes: number;
    preventUninstall: boolean;
    preventServiceStop: boolean;
    preventModification: boolean;
  }>((a) => ({
    cmd: "enable_protection",
    args: {
      list_id: a.listId,
      duration_minutes: a.minutes,
      prevent_uninstall: a.preventUninstall,
      prevent_service_stop: a.preventServiceStop,
      prevent_modification: a.preventModification,
    },
  }));

// ─── Settings ───────────────────────────────────────────────────────

export function useSetting(key: string, fallback?: string) {
  return useQuery({
    queryKey: queryKeys.setting(key),
    queryFn: async () =>
      expect(await run({ cmd: "get_setting", args: { key, default: fallback ?? null } }), "setting")
        .data,
  });
}

export function useSetSetting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { key: string; value: string }) => run({ cmd: "set_setting", args }),
    onSuccess: (_data, { key }) => qc.invalidateQueries({ queryKey: queryKeys.setting(key) }),
  });
}

export function useResetSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => run({ cmd: "reset_settings" }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["setting"] });
      // Retention lives in the same table and is wiped too, but it has its own key.
      qc.invalidateQueries({ queryKey: queryKeys.retention });
    },
  });
}

// ─── Enforcement ────────────────────────────────────────────────────

export const useApplyBlocks = () => useBlockListMutation<void>(() => ({ cmd: "apply_blocks" }));
export const useRemoveBlocks = () => useBlockListMutation<void>(() => ({ cmd: "remove_blocks" }));

// ─── Pomodoro ───────────────────────────────────────────────────────

export function usePomodoroStatus() {
  return useQuery({
    queryKey: queryKeys.pomodoro,
    queryFn: async () => expect(await run({ cmd: "pomodoro_status" }), "pomodoro_status").data,
    // A countdown that only moves every 2s looks broken; this one ticks.
    refetchInterval: 1000,
  });
}

/**
 * The built-in duration presets, straight from Rust.
 *
 * They never change while the app is open, so this is fetched once and kept —
 * refetching constants would only add flicker.
 */
export function usePomodoroPresets() {
  return useQuery({
    queryKey: ["pomodoro-presets"] as const,
    queryFn: async () => expect(await run({ cmd: "pomodoro_presets" }), "pomodoro_presets").data,
    staleTime: Number.POSITIVE_INFINITY,
  });
}

function usePomodoroMutation<TArgs>(build: (args: TArgs) => Command) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: TArgs) => run(build(args)),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.pomodoro });
      // A session toggles its block list, so those change too.
      qc.invalidateQueries({ queryKey: queryKeys.blockLists });
    },
  });
}

export const useStartPomodoro = () =>
  usePomodoroMutation<{ blockListId: string; config: PomodoroConfig }>(
    ({ blockListId, config }) => ({
      cmd: "pomodoro_start",
      args: { block_list_id: blockListId, config },
    }),
  );

export const usePausePomodoro = () => usePomodoroMutation<void>(() => ({ cmd: "pomodoro_pause" }));
export const useResumePomodoro = () =>
  usePomodoroMutation<void>(() => ({ cmd: "pomodoro_resume" }));
export const useSkipPomodoro = () => usePomodoroMutation<void>(() => ({ cmd: "pomodoro_skip" }));
export const useStopPomodoro = () => usePomodoroMutation<void>(() => ({ cmd: "pomodoro_stop" }));

export function usePomodoroHistory(days: number) {
  return useQuery({
    queryKey: ["pomodoro-history", days] as const,
    queryFn: async () =>
      expect(await run({ cmd: "pomodoro_history", args: { days } }), "pomodoro_history").data,
  });
}

// ─── Allowances ─────────────────────────────────────────────────────

export function useAllowances() {
  return useQuery({
    queryKey: queryKeys.allowances,
    queryFn: async () => expect(await run({ cmd: "allowance_list" }), "allowances").data,
  });
}

function useAllowanceMutation<TArgs>(build: (args: TArgs) => Command) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: TArgs) => run(build(args)),
    onSuccess: () => qc.invalidateQueries({ queryKey: queryKeys.allowances }),
  });
}

export const useCreateAllowance = () =>
  useAllowanceMutation<{ target: AllowanceMatch; dailyLimitSecs: number; strictMode: boolean }>(
    ({ target, dailyLimitSecs, strictMode }) => ({
      cmd: "allowance_create",
      args: { target, daily_limit_secs: dailyLimitSecs, strict_mode: strictMode },
    }),
  );

export const useUpdateAllowance = () =>
  useAllowanceMutation<{
    id: string;
    dailyLimitSecs: number;
    strictMode: boolean;
    enabled: boolean;
  }>((a) => ({
    cmd: "allowance_update",
    args: {
      id: a.id,
      daily_limit_secs: a.dailyLimitSecs,
      strict_mode: a.strictMode,
      enabled: a.enabled,
    },
  }));

export const useDeleteAllowance = () =>
  useAllowanceMutation<string>((id) => ({ cmd: "allowance_delete", args: { id } }));

export const useResetAllowanceToday = () =>
  useAllowanceMutation<string>((id) => ({ cmd: "allowance_reset_today", args: { id } }));
