/**
 * Typed command helpers and their TanStack Query bindings.
 *
 * The `Command` / `CommandResult` types here mirror the Rust enums by hand
 * *for now*. Once `tauri-specta` codegen is wired they are replaced by the
 * generated `src/bindings.ts`, and the mirror is deleted — hand-maintained
 * duplicates of a backend type are exactly the drift this project is removing.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { send } from "./transport";

// ─── Domain types (mirrors focuser-common) ──────────────────────────

export interface BlockList {
  id: string;
  name: string;
  enabled: boolean;
  websites: unknown[];
  applications: unknown[];
  exceptions: unknown[];
  created_at: string;
  updated_at: string;
}

// ─── Wire shapes (mirrors focuser-app::Command / CommandResult) ──────

type Command =
  | { cmd: "list_block_lists" }
  | { cmd: "create_block_list"; args: { name: string } }
  | { cmd: "update_block_list"; args: { list: BlockList } }
  | { cmd: "delete_block_list"; args: { id: string } }
  | { cmd: "toggle_block_list"; args: { id: string; enabled: boolean } };

type CommandResult =
  | { kind: "unit" }
  | { kind: "block_list"; data: BlockList }
  | { kind: "block_lists"; data: BlockList[] };

/**
 * Narrow a result to an expected variant.
 *
 * A mismatch means the frontend and backend disagree about a command's return
 * type, which is a bug worth failing loudly on rather than rendering `undefined`.
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
