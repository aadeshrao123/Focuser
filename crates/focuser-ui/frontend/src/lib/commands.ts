/**
 * Typed command helpers and their TanStack Query bindings.
 *
 * `Command`, `CommandResult`, and every domain type are **generated** from Rust
 * by tauri-specta into `src/bindings.ts`. Nothing here restates a backend type —
 * change a Rust struct or add a `Command` variant and this file stops compiling
 * until it is updated, which is the whole point.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { BlockList, Command, CommandResult } from "@/bindings";
import { send } from "./transport";

export type { BlockList, Command, CommandResult };

/**
 * Narrow a result to an expected variant.
 *
 * A mismatch means the frontend and backend disagree about a command's return
 * type — worth failing loudly on rather than rendering `undefined`.
 */
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
};

// ─── Hooks ──────────────────────────────────────────────────────────

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

export function useUpdateBlockList() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (list: BlockList) => run({ cmd: "update_block_list", args: { list } }),
    onSuccess: () => qc.invalidateQueries({ queryKey: queryKeys.blockLists }),
  });
}

export function useToggleBlockList() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: string; enabled: boolean }) =>
      run({ cmd: "toggle_block_list", args: vars }),
    onSuccess: () => qc.invalidateQueries({ queryKey: queryKeys.blockLists }),
  });
}

export function useDeleteBlockList() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => run({ cmd: "delete_block_list", args: { id } }),
    onSuccess: () => qc.invalidateQueries({ queryKey: queryKeys.blockLists }),
  });
}
