import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { type ReactNode, StrictMode } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BlockList, Command, CommandResult, ScheduledLockState } from "@/bindings";
import { TooltipProvider } from "@/components/ui/tooltip";
import { cellKey, cellsToSlots, DAYS, HOURS } from "@/lib/schedule";
import { CommandError } from "@/lib/transport";
import { Allowances } from "./allowances";
import { BlockLists } from "./block-lists";
import { Schedule } from "./schedule";

const send = vi.hoisted(() => vi.fn());
vi.mock("@/lib/transport", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/transport")>()),
  send,
}));
// The list picker's Radix mechanics have no bearing on what is tested here.
vi.mock("@/components/ui/select", () => ({
  Select: (p: {
    value: string;
    onValueChange: (v: string) => void;
    options: { value: string; label: string }[];
    "aria-label"?: string;
    id?: string;
  }) => (
    <select
      id={p.id}
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

// ─── A stand-in for the command core ────────────────────────────────

const list = (id: string, name: string): BlockList => ({
  id,
  name,
  enabled: true,
  websites: [],
  applications: [],
  exceptions: [],
  protection: null,
  lock: null,
  scheduled_protection: null,
  schedule_unlocked_until: null,
  schedule: {
    id: `schedule-${id}`,
    name,
    enabled: true,
    time_slots: [{ day: "Mon", start: "09:00:00", end: "17:00:00" }],
  },
  breaks: null,
  shared_allowance: null,
  created_at: "2026-09-21T00:00:00Z",
  updated_at: "2026-09-21T00:00:00Z",
});

const everyHour = () =>
  cellsToSlots(new Set(DAYS.flatMap((day) => HOURS.map((hour) => cellKey(day, hour)))));

let lists: BlockList[];
/** The list every test is about. */
let stored: BlockList;
let state: ScheduledLockState;
/** Whether the list is inside its hours right now. */
let active: boolean;
let remaining: number;
/** A command the core refuses, the way it does for a locked list. */
let refuse: Command["cmd"] | null;

beforeEach(() => {
  vi.clearAllMocks();
  stored = list("list-1", "Weekly");
  lists = [stored];
  state = "off";
  active = false;
  remaining = 1800;
  refuse = null;

  send.mockImplementation(async (cmd: Command): Promise<CommandResult> => {
    if (cmd.cmd === refuse) throw new CommandError({ code: "protected", message: "protected" });

    switch (cmd.cmd) {
      case "list_block_lists":
        return { kind: "block_lists", data: structuredClone(lists) };
      case "get_scheduled_protection_status":
        return {
          kind: "scheduled_protection_status",
          data: lists.map((l) => ({
            block_list_id: l.id,
            state: l.id === stored.id ? state : "off",
          })),
        };
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
        state = cmd.args.enabled
          ? state === "unlocked_for_editing"
            ? state
            : active
              ? "locked"
              : "inactive"
          : "off";
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
      case "update_schedule":
        stored.schedule = cmd.args.always_active
          ? null
          : { id: "s", name: stored.name, enabled: true, time_slots: cmd.args.slots };
        return { kind: "unit" };
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
      default:
        throw new Error(`Unexpected command: ${cmd.cmd}`);
    }
  });
});

/** Both pages side by side, which is how the same setting is seen from each. */
function show(
  children: ReactNode = (
    <>
      <div data-testid="lists">
        <BlockLists />
      </div>
      <div data-testid="schedule">
        <Schedule />
      </div>
    </>
  ),
  at = "/schedule",
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[at]}>
        <TooltipProvider>{children}</TooltipProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const page = (name: string) => within(screen.getByTestId(name));
const lockSwitch = (name: string) =>
  page(name).getByRole("switch", { name: "Lock during schedule: Weekly" });
const shareSwitch = (name: string) =>
  page(name).getByRole("switch", { name: "Shared allowance: Weekly" });
const sent = (name: Command["cmd"]) => send.mock.calls.filter(([c]) => c.cmd === name);

// ─── The card ───────────────────────────────────────────────────────

describe("a list card", () => {
  it("shows the two options as switches, with no setup on the page", async () => {
    show();
    await waitFor(() => expect(lockSwitch("lists")).toBeEnabled());

    expect(shareSwitch("lists")).toBeEnabled();
    // The unlock method used to be laid out under every list, all the time.
    expect(page("lists").queryByRole("radio")).toBeNull();
    expect(page("lists").queryByRole("spinbutton")).toBeNull();
  });

  it("says its hours in a line and links to the schedule for that list", async () => {
    show();

    expect(await page("lists").findByText("Mon 9am–5pm")).toBeVisible();
    expect(page("lists").getByRole("link", { name: "Edit schedule" })).toHaveAttribute(
      "href",
      "/schedule?list=list-1",
    );
  });
});

describe("a list with no schedule to follow", () => {
  it.each([
    ["no schedule at all", () => null],
    [
      "a schedule with every hour set",
      () => ({ id: "s", name: "Weekly", enabled: true, time_slots: everyHour() }),
    ],
  ])("offers the way to set one instead of an error: %s", async (_case, schedule) => {
    stored.schedule = schedule();
    show();

    expect(
      await page("lists").findByText("On all week. Both options need a schedule with a gap."),
    ).toBeVisible();
    expect(lockSwitch("lists")).toBeDisabled();
    expect(shareSwitch("lists")).toBeDisabled();
    // Straight to the hours of this list, with the grid already open.
    expect(page("lists").getByRole("link", { name: "Set schedule" })).toHaveAttribute(
      "href",
      "/schedule?list=list-1&set=hours",
    );
    expect(page("lists").queryByRole("link", { name: "Edit schedule" })).toBeNull();
  });

  it("can still switch off a lock that was left on it", async () => {
    stored.schedule = null;
    stored.scheduled_protection = { lock: null };
    state = "inactive";
    show();
    await waitFor(() => expect(lockSwitch("lists")).toBeEnabled());

    fireEvent.click(lockSwitch("lists"));

    await waitFor(() => expect(stored.scheduled_protection).toBeNull());
  });
});

// ─── Turning the lock on ────────────────────────────────────────────

describe.each(["lists", "schedule"])("turning the lock on from %s", (name) => {
  const dialog = () =>
    within(screen.getByRole("dialog", { name: "Lock Weekly during its schedule" }));

  it("asks how to unlock first, and saves nothing until that is answered", async () => {
    show();
    await waitFor(() => expect(lockSwitch(name)).toBeEnabled());

    fireEvent.click(lockSwitch(name));
    expect(dialog().getByRole("radio", { name: "Type random text" })).toBeChecked();
    fireEvent.click(dialog().getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(sent("configure_scheduled_protection")).toHaveLength(0);
    expect(lockSwitch(name)).not.toBeChecked();
  });

  it("saves the method that was chosen, and both pages show the lock as on", async () => {
    show();
    await waitFor(() => expect(lockSwitch(name)).toBeEnabled());

    fireEvent.click(lockSwitch(name));
    fireEvent.click(dialog().getByRole("button", { name: "Turn on lock" }));

    await waitFor(() => expect(lockSwitch("lists")).toBeChecked());
    expect(lockSwitch("schedule")).toBeChecked();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(send).toHaveBeenCalledWith({
      cmd: "configure_scheduled_protection",
      args: { list_id: "list-1", enabled: true, lock: { kind: "random_text", length: 16 } },
    });
    expect(page(name).getByText("Random text · 16 characters")).toBeVisible();
    expect(page(name).getByText("Locks when the schedule starts")).toBeVisible();
  });

  it("turns it off again with one click", async () => {
    stored.scheduled_protection = { lock: { RandomText: { length: 16 } } };
    state = "inactive";
    show();
    // Checked comes with the list; enabled waits for the lock's state as well.
    await waitFor(() => expect(lockSwitch(name)).toBeEnabled());
    expect(lockSwitch(name)).toBeChecked();

    fireEvent.click(lockSwitch(name));

    await waitFor(() => expect(stored.scheduled_protection).toBeNull());
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("the unlock method", () => {
  const dialog = () => within(screen.getByRole("dialog"));
  async function open() {
    show(<BlockLists />);
    const toggle = await screen.findByRole("switch", { name: "Lock during schedule: Weekly" });
    await waitFor(() => expect(toggle).toBeEnabled());
    fireEvent.click(toggle);
  }

  it("can be no early unlock at all", async () => {
    await open();
    fireEvent.click(dialog().getByRole("radio", { name: "None — wait it out" }));
    fireEvent.click(dialog().getByRole("button", { name: "Turn on lock" }));

    await waitFor(() => expect(stored.scheduled_protection).toEqual({ lock: null }));
    expect(await screen.findByText("No early unlock")).toBeVisible();
  });

  it("can be a password, once it is typed the same twice", async () => {
    await open();
    fireEvent.click(dialog().getByRole("radio", { name: "Password" }));
    const turnOn = dialog().getByRole("button", { name: "Turn on lock" });

    expect(turnOn).toBeDisabled();
    fireEvent.change(dialog().getByPlaceholderText("Choose a password"), {
      target: { value: "secret" },
    });
    fireEvent.change(dialog().getByPlaceholderText("Confirm password"), {
      target: { value: "secret" },
    });
    fireEvent.click(turnOn);

    await waitFor(() =>
      expect(send).toHaveBeenCalledWith({
        cmd: "configure_scheduled_protection",
        args: { list_id: "list-1", enabled: true, lock: { kind: "password", password: "secret" } },
      }),
    );
  });

  it("can be changed later, from the same pop-up, while the list is not locked", async () => {
    stored.scheduled_protection = { lock: { RandomText: { length: 16 } } };
    state = "unlocked_for_editing";
    show(<BlockLists />);

    fireEvent.click(await screen.findByRole("button", { name: "Change unlock method" }));
    const length = dialog().getByRole("spinbutton", { name: "Challenge length" });
    fireEvent.change(length, { target: { value: "48" } });
    fireEvent.blur(length);
    fireEvent.click(dialog().getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(stored.scheduled_protection).toEqual({ lock: { RandomText: { length: 48 } } }),
    );
    expect(sent("configure_scheduled_protection").every(([c]) => c.args.enabled)).toBe(true);
  });

  it("stays inside the pop-up when the core refuses it", async () => {
    await open();
    refuse = "configure_scheduled_protection";
    fireEvent.click(dialog().getByRole("button", { name: "Turn on lock" }));

    expect(
      await dialog().findByText(
        "A lock is running on this list, so it cannot be changed until the lock expires.",
      ),
    ).toBeVisible();
    expect(dialog().getByRole("button", { name: "Turn on lock" })).toBeEnabled();
  });
});

// ─── A lock that is running ─────────────────────────────────────────

describe.each(["lists", "schedule"])("a locked list on %s", (name) => {
  beforeEach(() => {
    stored.scheduled_protection = { lock: { RandomText: { length: 6 } } };
    state = "locked";
    active = true;
  });

  it("cannot be switched off, has no settings to change, and offers to unlock", async () => {
    show();
    await page(name).findByRole("button", { name: "Unlock for editing" });

    expect(lockSwitch(name)).toBeDisabled();
    expect(page(name).queryByRole("button", { name: "Change unlock method" })).toBeNull();
    expect(page("lists").queryByRole("link", { name: "Edit schedule" })).toBeNull();
  });

  it("unlocks from a pop-up by typing the text, and locks again with one click", async () => {
    show();
    fireEvent.click(await page(name).findByRole("button", { name: "Unlock for editing" }));

    const dialog = within(screen.getByRole("dialog", { name: "Unlock Weekly early" }));
    await dialog.findByText("abcdef");
    fireEvent.change(dialog.getByPlaceholderText("Type the string above"), {
      target: { value: "abcdef" },
    });
    fireEvent.click(dialog.getByRole("button", { name: /^Unlock$/ }));

    await page("lists").findByText("UNLOCKED FOR EDITING");
    expect(page("schedule").getByText("UNLOCKED FOR EDITING")).toBeVisible();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(sent("unlock_protection")).toHaveLength(1);

    fireEvent.click(page(name).getByRole("button", { name: "Lock again now" }));

    await page(name).findByRole("button", { name: "Unlock for editing" });
    expect(send).toHaveBeenCalledWith({
      cmd: "relock_scheduled_protection",
      args: { list_id: "list-1" },
    });
  });

  it("says so when there is no way to unlock early", async () => {
    stored.scheduled_protection = { lock: null };
    show();

    expect(await page(name).findByText("No early unlock")).toBeVisible();
    expect(page(name).queryByRole("button", { name: "Unlock for editing" })).toBeNull();
  });
});

// ─── The shared allowance ───────────────────────────────────────────

describe("the shared allowance", () => {
  it("switches on at 30 minutes, takes a new length, and switches off", async () => {
    show(<BlockLists />);
    const toggle = () => screen.getByRole("switch", { name: "Shared allowance: Weekly" });
    await waitFor(() => expect(toggle()).toBeEnabled());

    fireEvent.click(toggle());
    await waitFor(() => expect(stored.shared_allowance?.minutes).toBe(30));

    const minutes = await screen.findByRole("spinbutton", { name: "Shared allowance minutes" });
    fireEvent.change(minutes, { target: { value: "15" } });
    fireEvent.blur(minutes);
    await waitFor(() => expect(stored.shared_allowance?.minutes).toBe(15));

    fireEvent.click(toggle());
    await waitFor(() => expect(stored.shared_allowance).toBeNull());
  });

  it.each([
    [1080, "18 min left"],
    [0, "Used up"],
  ])("says what is left while its hours run: %s seconds", async (seconds, label) => {
    stored.shared_allowance = { minutes: 30 };
    active = true;
    remaining = seconds;
    show(<BlockLists />);

    expect(await screen.findByText(label)).toBeVisible();
  });

  it("says nothing about time left outside its hours", async () => {
    stored.shared_allowance = { minutes: 30 };
    remaining = 0;
    show(<BlockLists />);
    await screen.findByRole("spinbutton", { name: "Shared allowance minutes" });

    expect(screen.queryByText("Used up")).toBeNull();
  });

  it("cannot be changed while the list is locked", async () => {
    stored.shared_allowance = { minutes: 30 };
    stored.scheduled_protection = { lock: null };
    state = "locked";
    show(<BlockLists />);
    await screen.findByText("No early unlock");

    expect(screen.getByRole("switch", { name: "Shared allowance: Weekly" })).toBeDisabled();
    expect(screen.getByRole("spinbutton", { name: "Shared allowance minutes" })).toBeDisabled();
  });

  it("is on the schedule page as well, next to the hours it follows", async () => {
    show();
    await waitFor(() => expect(shareSwitch("schedule")).toBeEnabled());

    fireEvent.click(shareSwitch("schedule"));

    await waitFor(() => expect(shareSwitch("lists")).toBeChecked());
  });

  it("marks a site allowance as paused while it runs", async () => {
    show(<Allowances />);

    expect(await screen.findByText("Paused while shared allowance is active.")).toBeVisible();
  });
});

// ─── When the core says no ──────────────────────────────────────────

describe("a change the core refuses", () => {
  it("shows as a pop-up that goes away when dismissed, not as a line on the page", async () => {
    refuse = "configure_shared_allowance";
    show(<BlockLists />);
    const toggle = await screen.findByRole("switch", { name: "Shared allowance: Weekly" });
    await waitFor(() => expect(toggle).toBeEnabled());

    fireEvent.click(toggle);

    const message = within(await screen.findByRole("alertdialog"));
    expect(
      message.getByText(
        "A lock is running on this list, so it cannot be changed until the lock expires.",
      ),
    ).toBeVisible();
    fireEvent.click(message.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("a new list the core refuses", () => {
  it("is told in the same pop-up, so the page has one way of saying no", async () => {
    refuse = "create_block_list";
    show(<BlockLists />);

    fireEvent.change(await screen.findByLabelText("New block list name"), {
      target: { value: "Evenings" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByRole("alertdialog")).toBeVisible();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("a number the core refuses", () => {
  it("goes back to the saved one, so the field does not show what was not saved", async () => {
    stored.shared_allowance = { minutes: 30 };
    refuse = "configure_shared_allowance";
    show(<BlockLists />);
    const field = () => screen.getByRole("spinbutton", { name: "Shared allowance minutes" });
    await waitFor(() => expect(field()).toBeEnabled());

    fireEvent.change(field(), { target: { value: "45" } });
    fireEvent.blur(field());

    await screen.findByRole("alertdialog");
    expect(field()).toHaveValue(30);
  });
});

// ─── The schedule page ──────────────────────────────────────────────

describe("the schedule page", () => {
  it("opens on the list named in the address", async () => {
    lists = [list("list-0", "Other"), stored];
    show(<Schedule />, "/schedule?list=list-1");

    expect(
      await screen.findByRole("switch", { name: "Lock during schedule: Weekly" }),
    ).toBeVisible();
    expect(screen.queryByRole("switch", { name: "Lock during schedule: Other" })).toBeNull();
  });

  it("opens the hours grid for a list that is on all week when asked to set hours", async () => {
    stored.schedule = null;
    show(<Schedule />, "/schedule?list=list-1&set=hours");

    // The presets only exist on the grid, not on the "always active" panel.
    expect(await screen.findByRole("button", { name: "Work hours" })).toBeVisible();
    expect(
      screen.getByText("Both options need a schedule with a gap. Set the hours above and save."),
    ).toBeVisible();
  });

  it("lands on that grid from the Set schedule button of a list", async () => {
    stored.schedule = null;
    lists = [list("list-0", "Other"), stored];
    // The app runs in StrictMode, which runs every effect twice in development.
    // Coming from the list page, the lists are already loaded when this page
    // mounts, so whatever opens the grid has to survive that second run.
    show(
      <StrictMode>
        <Routes>
          <Route path="/block-lists" element={<BlockLists />} />
          <Route path="/schedule" element={<Schedule />} />
        </Routes>
      </StrictMode>,
      "/block-lists",
    );
    const cards = await screen.findAllByTestId("block-list-row");
    const weekly = cards.find((c) => within(c).queryByText("Weekly"));
    if (!weekly) throw new Error("no card for Weekly");

    fireEvent.click(within(weekly).getByRole("link", { name: "Set schedule" }));

    expect(await screen.findByRole("button", { name: "Work hours" })).toBeVisible();
    expect(screen.getByRole("switch", { name: "Lock during schedule: Weekly" })).toBeVisible();
  });

  it("opens such a list on the always-active panel when it was not asked for hours", async () => {
    stored.schedule = null;
    show(<Schedule />, "/schedule?list=list-1");
    await screen.findByRole("switch", { name: "Lock during schedule: Weekly" });

    expect(screen.queryByRole("button", { name: "Work hours" })).toBeNull();
  });

  it("stops a save that would leave a locking list with no gap, and says why", async () => {
    stored.scheduled_protection = { lock: { RandomText: { length: 16 } } };
    state = "inactive";
    show(<Schedule />);

    fireEvent.click(await screen.findByRole("button", { name: "Every hour" }));
    fireEvent.click(screen.getByRole("button", { name: "Save schedule" }));

    const message = within(screen.getByRole("alertdialog", { name: "This schedule needs a gap" }));
    expect(message.getByText(/Weekly locks during its schedule/)).toBeVisible();
    expect(sent("update_schedule")).toHaveLength(0);
    fireEvent.click(message.getByRole("button", { name: "OK" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("saves hours with no gap for a list that has neither option", async () => {
    show(<Schedule />);

    fireEvent.click(await screen.findByRole("button", { name: "Every hour" }));
    fireEvent.click(screen.getByRole("button", { name: "Save schedule" }));

    await waitFor(() => expect(sent("update_schedule")).toHaveLength(1));
  });
});

// ─── The manual lock ────────────────────────────────────────────────

describe("the manual lock", () => {
  it("is set up in a pop-up, and locks with no early unlock unless one is chosen", async () => {
    show(<BlockLists />);
    fireEvent.click(await screen.findByRole("button", { name: "Protect Weekly" }));

    const dialog = within(screen.getByRole("dialog", { name: "Lock Weekly for a set time" }));
    expect(dialog.getByRole("radio", { name: "None — wait it out" })).toBeChecked();
    fireEvent.click(dialog.getByRole("button", { name: /Lock for/ }));

    await waitFor(() =>
      expect(send).toHaveBeenCalledWith({
        cmd: "enable_protection",
        args: {
          list_id: "list-1",
          duration_minutes: 60,
          prevent_uninstall: true,
          prevent_service_stop: true,
          prevent_modification: true,
          lock: null,
        },
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("takes the unlock method from the same chooser as the scheduled lock", async () => {
    show(<BlockLists />);
    fireEvent.click(await screen.findByRole("button", { name: "Protect Weekly" }));
    const dialog = within(screen.getByRole("dialog"));

    fireEvent.click(dialog.getByRole("radio", { name: "Type random text" }));
    fireEvent.click(dialog.getByRole("button", { name: /Lock for/ }));

    await waitFor(() =>
      expect(send).toHaveBeenCalledWith(
        expect.objectContaining({
          cmd: "enable_protection",
          args: expect.objectContaining({ lock: { kind: "random_text", length: 16 } }),
        }),
      ),
    );
  });

  it("can lock until unlocked, but only with a way to unlock", async () => {
    show(<BlockLists />);
    fireEvent.click(await screen.findByRole("button", { name: "Protect Weekly" }));
    const dialog = within(screen.getByRole("dialog"));

    fireEvent.click(dialog.getByRole("switch", { name: "Until I unlock it" }));
    const lock = dialog.getByRole("button", { name: "Lock until unlocked" });
    expect(lock).toBeDisabled();

    fireEvent.click(dialog.getByRole("radio", { name: "Type random text" }));
    expect(lock).toBeEnabled();
    fireEvent.click(lock);

    await waitFor(() =>
      expect(send).toHaveBeenCalledWith(
        expect.objectContaining({
          cmd: "enable_protection",
          args: expect.objectContaining({
            duration_minutes: null,
            lock: { kind: "random_text", length: 16 },
          }),
        }),
      ),
    );
  });
});
