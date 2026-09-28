import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { BlockList, Command, CommandResult, ScheduledLockState } from "@/bindings";
import { ProtectForm } from "@/components/focus-lock-forms";
import { TooltipProvider } from "@/components/ui/tooltip";
import { BlockLists } from "./block-lists";
import { Schedule } from "./schedule";

const send = vi.hoisted(() => vi.fn());
vi.mock("@/lib/transport", () => ({ send }));
// Exercise page/query/mutation wiring; the select's Radix mechanics have no bearing on it.
vi.mock("@/components/ui/select", () => ({
  Select: (p: {
    value: string;
    onValueChange: (v: string) => void;
    options: { value: string; label: string }[];
    "aria-label"?: string;
  }) => (
    <select
      aria-label={p["aria-label"]}
      value={p.value}
      onChange={(e) => p.onValueChange(e.target.value)}
    >
      {p.options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

let stored: BlockList;
let state: ScheduledLockState;
let active: boolean;
beforeEach(() => {
  vi.clearAllMocks();
  active = false;
  state = "off";
  stored = {
    id: "list-1",
    name: "Weekly",
    enabled: true,
    websites: [],
    applications: [],
    exceptions: [],
    protection: null,
    lock: null,
    scheduled_protection: null,
    schedule_unlocked_until: null,
    schedule: {
      id: "schedule-1",
      name: "Weekly",
      enabled: true,
      time_slots: [{ day: "Mon", start: "09:00:00", end: "17:00:00" }],
    },
    breaks: null,
    created_at: "2026-09-21T00:00:00Z",
    updated_at: "2026-09-21T00:00:00Z",
  };
  send.mockImplementation(async (cmd: Command): Promise<CommandResult> => {
    switch (cmd.cmd) {
      case "list_block_lists":
        return { kind: "block_lists", data: [structuredClone(stored)] };
      case "get_scheduled_protection_status":
        return { kind: "scheduled_protection_status", data: [{ block_list_id: stored.id, state }] };
      case "get_protection_status":
        return {
          kind: "protection_status",
          data:
            state === "locked"
              ? [
                  {
                    block_list_id: stored.id,
                    block_list_name: stored.name,
                    prevent_modification: true,
                    prevent_service_stop: true,
                    prevent_uninstall: true,
                    remaining_seconds: 3600,
                    expires_at: "2099-09-21T17:00:00Z",
                  },
                ]
              : [],
        };
      case "configure_scheduled_protection": {
        const lock = cmd.args.lock;
        stored.scheduled_protection = cmd.args.enabled
          ? {
              lock:
                lock?.kind === "password"
                  ? { Password: { hash: "hashed" } }
                  : lock?.kind === "random_text"
                    ? { RandomText: { length: lock.length } }
                    : null,
            }
          : null;
        state = cmd.args.enabled ? (active ? "locked" : "inactive") : "off";
        return { kind: "unit" };
      }
      case "request_unlock_challenge":
        return { kind: "text", data: "abcdef" };
      case "unlock_protection":
        state = "unlocked_for_editing";
        return { kind: "unit" };
      case "relock_scheduled_protection":
        state = "locked";
        return { kind: "unit" };
      case "enable_protection":
        return { kind: "unit" };
      default:
        throw new Error(`Unexpected command: ${cmd.cmd}`);
    }
  });
});

function show(
  children = (
    <>
      <div data-testid="lists">
        <BlockLists />
      </div>
      <div data-testid="schedule">
        <Schedule />
      </div>
    </>
  ),
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>{children}</TooltipProvider>
    </QueryClientProvider>,
  );
}
const page = (name: string) => within(screen.getByTestId(name));
const toggle = (name: string) => page(name).getByRole("switch", { name: "Lock during schedule" });

it.each(["lists", "schedule"])(
  "%s toggle persists immediately and updates the same setting on both pages",
  async (name) => {
    show();
    await waitFor(() => expect(toggle(name)).toBeEnabled());
    expect(toggle(name)).not.toBeChecked();
    fireEvent.click(toggle(name));
    await waitFor(() => expect(toggle("lists")).toBeChecked());
    expect(toggle("schedule")).toBeChecked();
    expect(stored.scheduled_protection?.lock).toEqual({ RandomText: { length: 16 } });
    expect(send).toHaveBeenCalledWith({
      cmd: "configure_scheduled_protection",
      args: { list_id: "list-1", enabled: true, lock: { kind: "random_text", length: 16 } },
    });
    expect(screen.queryByText("Save scheduled protection")).toBeNull();
    fireEvent.click(toggle(name));
    await waitFor(() => expect(toggle("lists")).not.toBeChecked());
    expect(toggle("schedule")).not.toBeChecked();
    expect(stored.scheduled_protection).toBeNull();
  },
);

it.each([
  ["off", "Scheduled protection OFF"],
  ["inactive", "Scheduled protection enabled"],
  ["locked", "LOCKED"],
  ["unlocked_for_editing", "UNLOCKED FOR EDITING"],
] as const)("both pages display backend state %s", async (value, label) => {
  state = value;
  if (state !== "off") stored.scheduled_protection = { lock: { RandomText: { length: 6 } } };
  show();
  await waitFor(() => expect(page("lists").getByText(label)).toBeVisible());
  expect(page("schedule").getByText(label)).toBeVisible();
  if (state === "locked") {
    expect(toggle("lists")).toBeDisabled();
    expect(toggle("schedule")).toBeDisabled();
  }
});

it("one unlock permits editing until Lock again now immediately restores both locked displays", async () => {
  state = "locked";
  stored.scheduled_protection = { lock: { RandomText: { length: 6 } } };
  show();
  fireEvent.click(await page("lists").findByRole("button", { name: "Unlock for editing" }));
  await page("lists").findByText("abcdef");
  fireEvent.change(page("lists").getByPlaceholderText("Type the string above"), {
    target: { value: "abcdef" },
  });
  fireEvent.click(page("lists").getByRole("button", { name: /^Unlock$/ }));
  await page("schedule").findByText("UNLOCKED FOR EDITING");
  expect(page("lists").getByText("UNLOCKED FOR EDITING")).toBeVisible();
  fireEvent.click(page("schedule").getByRole("button", { name: "Lock again now" }));
  await page("lists").findByText("LOCKED");
  expect(page("schedule").getByText("LOCKED")).toBeVisible();
  expect(send.mock.calls.filter(([c]) => c.cmd === "unlock_protection")).toHaveLength(1);
  expect(send).toHaveBeenCalledWith({
    cmd: "relock_scheduled_protection",
    args: { list_id: "list-1" },
  });
});

it("password setup must be complete before the toggle enables protection", async () => {
  show();
  await waitFor(() => expect(toggle("schedule")).toBeEnabled());
  fireEvent.change(page("schedule").getByRole("combobox", { name: "Unlock method" }), {
    target: { value: "password" },
  });
  expect(toggle("schedule")).toBeDisabled();
  const inputs = page("schedule").getAllByPlaceholderText(/password/i);
  fireEvent.change(inputs[0], { target: { value: "secret" } });
  fireEvent.change(inputs[1], { target: { value: "different" } });
  expect(toggle("schedule")).toBeDisabled();
  fireEvent.change(inputs[1], { target: { value: "secret" } });
  fireEvent.click(toggle("schedule"));
  await waitFor(() => expect(stored.scheduled_protection?.lock).toHaveProperty("Password"));
  expect(send).toHaveBeenCalledWith({
    cmd: "configure_scheduled_protection",
    args: { list_id: "list-1", enabled: true, lock: { kind: "password", password: "secret" } },
  });
});

it("keeps supported random-text bounds and permits None", async () => {
  show();
  await waitFor(() => expect(toggle("lists")).toBeEnabled());
  const length = page("lists").getByRole("spinbutton", { name: "Challenge length" });
  expect(length).toHaveAttribute("min", "6");
  expect(length).toHaveAttribute("max", "64");
  fireEvent.change(page("lists").getByRole("combobox", { name: "Unlock method" }), {
    target: { value: "none" },
  });
  active = true;
  fireEvent.click(toggle("lists"));
  await page("lists").findByText("LOCKED");
  expect(page("lists").getByRole("button", { name: "Unlock for editing" })).toBeDisabled();
  expect(stored.scheduled_protection?.lock).toBeNull();
});

it("preserves the manual timed Focus Lock action", async () => {
  show(<ProtectForm list={stored} onDone={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: /Lock for/ }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      expect.objectContaining({
        cmd: "enable_protection",
        args: expect.objectContaining({ duration_minutes: 60 }),
      }),
    ),
  );
});
