import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { BlockList, Command, CommandResult } from "@/bindings";
import { UnlockForm } from "./focus-lock-forms";

const send = vi.hoisted(() => vi.fn());
vi.mock("@/lib/transport", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/transport")>()),
  send,
}));

let challenge: string;
let rejectNext: boolean;
beforeEach(() => {
  vi.clearAllMocks();
  challenge = "AbcDef";
  rejectNext = false;
  send.mockImplementation(async (command: Command): Promise<CommandResult> => {
    if (command.cmd === "request_unlock_challenge") return { kind: "text", data: challenge };
    if (command.cmd === "unlock_protection") {
      if (rejectNext) {
        rejectNext = false;
        challenge = "GhijkL";
        throw new Error("Challenge rejected");
      }
      return { kind: "unit" };
    }
    throw new Error(`Unexpected command: ${command.cmd}`);
  });
});

async function show(password = false) {
  const list: BlockList = {
    id: "list-1",
    name: "Focus",
    enabled: true,
    websites: [],
    applications: [],
    exceptions: [],
    lock: null,
    protection: null,
    schedule: null,
    schedule_unlocked_until: null,
    shared_allowance: null,
    breaks: null,
    scheduled_protection: {
      lock: password ? { Password: { hash: "stored" } } : { RandomText: { length: 6 } },
    },
    created_at: "2026-09-28T00:00:00Z",
    updated_at: "2026-09-28T00:00:00Z",
  };
  const onDone = vi.fn();
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <UnlockForm list={list} onDone={onDone} />
    </QueryClientProvider>,
  );
  if (!password) await screen.findByText(challenge);
  const input = screen.getByLabelText(password ? "Enter the password" : "Type the string above");
  const button = screen.getByRole("button", { name: "Unlock" });
  return { input, button, onDone };
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } });
}
function unlockCalls() {
  return send.mock.calls.filter(([command]) => command.cmd === "unlock_protection");
}

it("correct prefixes show no mismatch and cannot unlock", async () => {
  const { input, button } = await show();
  for (const prefix of ["", "A", "Abc", "AbcDe"]) {
    type(input, prefix);
    expect(screen.queryByRole("status")).toBeNull();
    expect(button).toBeDisabled();
  }
});

it("reports the first mismatching character using one-based positions", async () => {
  const { input } = await show();
  type(input, "AbXXef");
  expect(screen.getByRole("status")).toHaveTextContent("Mismatch at character 3.");
  expect(input).toHaveAttribute("aria-invalid", "true");
  type(input, "abcDef");
  expect(screen.getByRole("status")).toHaveTextContent("Mismatch at character 1.");
});

it("correcting a mismatch removes the warning without clearing the input or fetching again", async () => {
  const { input } = await show();
  type(input, "AbX");
  expect(screen.getByRole("status")).toBeVisible();
  type(input, "AbcD");
  expect(screen.queryByRole("status")).toBeNull();
  expect(input).toHaveValue("AbcD");
  expect(send.mock.calls.filter(([c]) => c.cmd === "request_unlock_challenge")).toHaveLength(1);
});

it("reports the first extra character beyond the challenge", async () => {
  const { input, button } = await show();
  type(input, "AbcDefXY");
  expect(screen.getByRole("status")).toHaveTextContent("Mismatch at character 7.");
  expect(button).toBeDisabled();
});

it("enables random-text Unlock only for an exact match", async () => {
  const { input, button } = await show();
  type(input, "AbcDe");
  expect(button).toBeDisabled();
  type(input, "AbcDef");
  expect(button).toBeEnabled();
  type(input, "AbcDef ");
  expect(button).toBeDisabled();
  fireEvent.click(button);
  expect(unlockCalls()).toHaveLength(0);
});

it.each(["AbXDef", "Abc"])("Enter cannot submit random-text response %s", async (value) => {
  const { input, onDone } = await show();
  type(input, value);
  fireEvent.keyDown(input, { key: "Enter" });
  expect(unlockCalls()).toHaveLength(0);
  expect(onDone).not.toHaveBeenCalled();
});

it.each(["button", "Enter"])("exact random-text match submits through %s", async (method) => {
  const { input, button, onDone } = await show();
  type(input, "AbcDef");
  if (method === "Enter") fireEvent.keyDown(input, { key: "Enter" });
  else fireEvent.click(button);
  await waitFor(() => expect(onDone).toHaveBeenCalledOnce());
  expect(unlockCalls()).toEqual([
    [{ cmd: "unlock_protection", args: { list_id: "list-1", response: "AbcDef" } }],
  ]);
});

it.each(["button", "Enter"])(
  "password unlock still submits a non-empty response through %s",
  async (method) => {
    const { input, button, onDone } = await show(true);
    expect(button).toBeDisabled();
    expect(input).toHaveAttribute("type", "password");
    type(input, "a password");
    expect(button).toBeEnabled();
    expect(screen.queryByRole("status")).toBeNull();
    if (method === "Enter") fireEvent.keyDown(input, { key: "Enter" });
    else fireEvent.click(button);
    await waitFor(() => expect(onDone).toHaveBeenCalledOnce());
    expect(send).toHaveBeenCalledWith({
      cmd: "unlock_protection",
      args: { list_id: "list-1", response: "a password" },
    });
    expect(send.mock.calls.some(([c]) => c.cmd === "request_unlock_challenge")).toBe(false);
  },
);

it("retains the backend rejection fallback and requires the fresh challenge", async () => {
  const { input, button, onDone } = await show();
  rejectNext = true;
  type(input, "AbcDef");
  fireEvent.click(button);
  await screen.findByText("GhijkL");
  expect(input).toHaveValue("");
  expect(button).toBeDisabled();
  expect(onDone).not.toHaveBeenCalled();
  type(input, "AbcDef");
  fireEvent.keyDown(input, { key: "Enter" });
  expect(unlockCalls()).toHaveLength(1);
  type(input, "GhijkL");
  fireEvent.click(button);
  await waitFor(() => expect(onDone).toHaveBeenCalledOnce());
});
