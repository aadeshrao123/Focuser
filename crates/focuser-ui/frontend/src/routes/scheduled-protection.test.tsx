import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";
import type { BlockList, Command, CommandResult, ScheduledLockState } from "@/bindings";
import { ProtectForm } from "@/components/focus-lock-forms";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Allowances } from "./allowances";
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
    disabled?: boolean;
  }) => (
    <select
      aria-label={p["aria-label"]}
      value={p.value}
      disabled={p.disabled}
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
let remaining: number;
beforeEach(() => {
  vi.clearAllMocks();
  active = false;
  remaining = 1800;
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
    shared_allowance: null,
    created_at: "2026-09-21T00:00:00Z",
    updated_at: "2026-09-21T00:00:00Z",
  };
  send.mockImplementation(async (cmd: Command): Promise<CommandResult> => {
    switch (cmd.cmd) {
      case "get_shared_allowance_status":
        return {
          kind: "shared_allowance_status",
          data: stored.shared_allowance
            ? [
                {
                  block_list_id: stored.id,
                  active,
                  limit_secs: stored.shared_allowance.minutes * 60,
                  remaining_secs: remaining,
                },
              ]
            : [],
        };
      case "configure_shared_allowance":
        stored.shared_allowance = cmd.args.minutes === null ? null : { minutes: cmd.args.minutes };
        return { kind: "unit" };
      case "allowance_list":
        return {
          kind: "allowances",
          data: [
            {
              allowance: {
                id: "allowance-1",
                target: { kind: "Domain", value: "youtube.com" },
                daily_limit_secs: 600,
                strict_mode: true,
                enabled: true,
                created_at: stored.created_at,
              },
              used_today_secs: 100,
              remaining_secs: 500,
              exhausted: false,
              paused_by_shared: true,
            },
          ],
        };
      case "get_browser_status":
        return { kind: "browser_status", data: [] };
      case "get_blocking_health":
        return {
          kind: "blocking_health",
          data: {
            active_lists: 1,
            extension_connected: false,
            hosts_writable: false,
            extension_only_rules: false,
            app_usage_measurable: true,
          },
        };
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

it("shared allowance toggle and duration persist immediately on Block Lists", async () => {
  show();
  const control = () => page("lists").getByRole("switch", { name: "Shared allowance" });
  await waitFor(() => expect(control()).toBeEnabled());
  fireEvent.click(control());
  await waitFor(() => expect(stored.shared_allowance?.minutes).toBe(30));
  const minutes = await page("lists").findByRole("spinbutton", {
    name: "Shared allowance minutes",
  });
  fireEvent.change(minutes, { target: { value: "15" } });
  fireEvent.blur(minutes);
  await waitFor(() => expect(stored.shared_allowance?.minutes).toBe(15));
  expect(
    page("lists").getByText(
      "Individual site/app allowances are paused while shared allowance is active.",
    ),
  ).toBeVisible();
  fireEvent.click(control());
  await waitFor(() => expect(stored.shared_allowance).toBeNull());
});

it.each([
  [1080, "Shared allowance: 18m remaining of 30m"],
  [0, "Shared allowance: Used up for this scheduled block"],
])("shared allowance shows active remaining/exhausted status %s", async (seconds, label) => {
  stored.shared_allowance = { minutes: 30 };
  active = true;
  remaining = Number(seconds);
  show();
  expect(await screen.findByText(String(label))).toBeVisible();
});

it("shared configuration is disabled while protected and hides stale inactive usage", async () => {
  stored.shared_allowance = { minutes: 30 };
  remaining = 0;
  state = "locked";
  show();
  await page("lists").findByText("LOCKED");
  expect(page("lists").getByRole("switch", { name: "Shared allowance" })).toBeDisabled();
  expect(
    page("lists").getByRole("spinbutton", { name: "Shared allowance minutes" }),
  ).toBeDisabled();
  expect(screen.queryByText("Shared allowance: Used up for this scheduled block")).toBeNull();
});

it("individual allowance displays its paused state", async () => {
  show(<Allowances />);
  expect(await screen.findByText("Paused while shared allowance is active.")).toBeVisible();
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
      <MemoryRouter initialEntries={["/schedule"]}>
        <TooltipProvider>{children}</TooltipProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
const page = (name: string) => within(screen.getByTestId(name));
const toggle = (name: string) => page(name).getByRole("switch", { name: "Lock during schedule" });

it("Block Lists owns configuration while Schedule shows a compact default summary", async () => {
  show();
  await waitFor(() => expect(toggle("lists")).toBeEnabled());
  expect(page("lists").getByRole("combobox", { name: "Unlock method" })).toBeEnabled();
  expect(page("lists").getByRole("spinbutton", { name: "Challenge length" })).toHaveValue(16);
  expect(page("schedule").queryByRole("combobox", { name: "Unlock method" })).toBeNull();
  expect(page("schedule").queryByRole("spinbutton", { name: "Challenge length" })).toBeNull();
  expect(page("schedule").queryByPlaceholderText(/password/i)).toBeNull();
  expect(page("schedule").getByText("Random text · 16 characters")).toBeVisible();
  expect(page("schedule").getByRole("link", { name: "Manage lock settings" })).toHaveAttribute(
    "href",
    "/block-lists",
  );
});

it.each([
  [{ RandomText: { length: 32 } }, "Random text · 32 characters"],
  [{ Password: { hash: "hashed" } }, "Password"],
  [null, "No early unlock"],
] as const)(
  "Schedule summarizes the persisted method %j without configuration fields",
  async (lock, summary) => {
    stored.scheduled_protection = { lock };
    state = "inactive";
    show();
    expect(await page("schedule").findByText(summary)).toBeVisible();
    expect(page("schedule").queryByRole("combobox", { name: "Unlock method" })).toBeNull();
    expect(page("schedule").queryByRole("spinbutton", { name: "Challenge length" })).toBeNull();
    expect(page("schedule").queryByPlaceholderText(/password/i)).toBeNull();
    expect(page("lists").getByRole("combobox", { name: "Unlock method" })).toBeDisabled();
    if (lock && "RandomText" in lock) {
      expect(page("lists").getByRole("spinbutton", { name: "Challenge length" })).toHaveValue(32);
    }
    // Disabling uses the shared command and does not silently replace the saved method.
    await waitFor(() => expect(toggle("schedule")).toBeEnabled());
    fireEvent.click(toggle("schedule"));
    await waitFor(() => expect(stored.scheduled_protection).toBeNull());
    expect(send).toHaveBeenCalledWith({
      cmd: "configure_scheduled_protection",
      args: { list_id: stored.id, enabled: false, lock: null },
    });
  },
);

it("Manage lock settings navigates to Block Lists", async () => {
  show(
    <Routes>
      <Route path="/schedule" element={<Schedule />} />
      <Route path="/block-lists" element={<BlockLists />} />
    </Routes>,
  );
  fireEvent.click(await screen.findByRole("link", { name: "Manage lock settings" }));
  expect(await screen.findByRole("combobox", { name: "Unlock method" })).toBeVisible();
  expect(screen.getByRole("switch", { name: "Shared allowance" })).toBeVisible();
});

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

it.each(["lists", "schedule"])(
  "%s unlock permits editing until Lock again now restores both locked displays",
  async (name) => {
    state = "locked";
    stored.scheduled_protection = { lock: { RandomText: { length: 6 } } };
    show();
    fireEvent.click(await page(name).findByRole("button", { name: "Unlock for editing" }));
    await page(name).findByText("abcdef");
    fireEvent.change(page(name).getByPlaceholderText("Type the string above"), {
      target: { value: "abcdef" },
    });
    fireEvent.click(page(name).getByRole("button", { name: /^Unlock$/ }));
    await page("schedule").findByText("UNLOCKED FOR EDITING");
    expect(page("lists").getByText("UNLOCKED FOR EDITING")).toBeVisible();
    fireEvent.click(
      page(name === "lists" ? "schedule" : "lists").getByRole("button", { name: "Lock again now" }),
    );
    await page("lists").findByText("LOCKED");
    expect(page("schedule").getByText("LOCKED")).toBeVisible();
    expect(send.mock.calls.filter(([c]) => c.cmd === "unlock_protection")).toHaveLength(1);
    expect(send).toHaveBeenCalledWith({
      cmd: "relock_scheduled_protection",
      args: { list_id: "list-1" },
    });
  },
);

it("password setup must be complete before the toggle enables protection", async () => {
  show();
  await waitFor(() => expect(toggle("lists")).toBeEnabled());
  fireEvent.change(page("lists").getByRole("combobox", { name: "Unlock method" }), {
    target: { value: "password" },
  });
  expect(toggle("lists")).toBeDisabled();
  const inputs = page("lists").getAllByPlaceholderText(/password/i);
  fireEvent.change(inputs[0], { target: { value: "secret" } });
  fireEvent.change(inputs[1], { target: { value: "different" } });
  expect(toggle("lists")).toBeDisabled();
  fireEvent.change(inputs[1], { target: { value: "secret" } });
  fireEvent.click(toggle("lists"));
  await waitFor(() => expect(stored.scheduled_protection?.lock).toHaveProperty("Password"));
  expect(await page("schedule").findByText("Password")).toBeVisible();
  expect(page("schedule").queryByRole("combobox", { name: "Unlock method" })).toBeNull();
  expect(page("schedule").queryByPlaceholderText(/password/i)).toBeNull();
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
  expect(length).toHaveAttribute("max", "256");
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
