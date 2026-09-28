import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BlockList, ProtectionInfo } from "@/bindings";
import { TooltipProvider } from "@/components/ui/tooltip";
import { BlockLists, ProtectForm } from "./block-lists";

const mocks = vi.hoisted(() => ({
  configure: vi.fn(),
  protect: vi.fn(),
  unlock: vi.fn(),
  challenge: vi.fn(),
  lists: vi.fn(),
  status: vi.fn(),
}));
const mutation = (mutate = vi.fn()) => ({ mutate, isPending: false, error: null });
vi.mock("@/lib/commands", () => ({
  useConfigureScheduledProtection: () => mutation(mocks.configure),
  useEnableProtection: () => mutation(mocks.protect),
  useUnlockProtection: () => mutation(mocks.unlock),
  useRequestUnlockChallenge: () => ({ ...mutation(mocks.challenge), data: "abcdef" }),
  useBlockLists: mocks.lists,
  useProtectionStatus: mocks.status,
  useCreateBlockList: () => mutation(),
  useDeleteBlockList: () => mutation(),
  useToggleBlockList: () => mutation(),
}));

function list(patch: Partial<BlockList> = {}): BlockList {
  return {
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
    ...patch,
  };
}

beforeEach(() => vi.clearAllMocks());

describe("scheduled Focus Lock setup", () => {
  it("is opt-in and sends a recurring configuration instead of a timed lock", () => {
    render(<ProtectForm list={list()} scheduled onDone={vi.fn()} />);
    const toggle = screen.getByRole("switch", { name: "Protect while schedule is active" });
    expect(toggle).not.toBeChecked();
    expect(screen.queryByRole("combobox", { name: "Unlock method" })).toBeNull();
    fireEvent.click(toggle);
    expect(screen.getByRole("combobox", { name: "Unlock method" })).toHaveTextContent("None");
    fireEvent.click(screen.getByRole("button", { name: "Save scheduled protection" }));
    expect(mocks.configure).toHaveBeenCalledWith(
      { listId: "list-1", enabled: true, lock: null },
      expect.any(Object),
    );
    expect(mocks.protect).not.toHaveBeenCalled();
  });

  it("reuses random-text configuration and its existing 6–64 character limits", () => {
    render(
      <ProtectForm
        list={list({ scheduled_protection: { lock: { RandomText: { length: 32 } } } })}
        scheduled
        onDone={vi.fn()}
      />,
    );
    const length = screen.getByRole("spinbutton", { name: "Challenge length" });
    expect(length).toHaveValue(32);
    expect(length).toHaveAttribute("min", "6");
    expect(length).toHaveAttribute("max", "64");
    fireEvent.click(screen.getByRole("button", { name: "Save scheduled protection" }));
    expect(mocks.configure).toHaveBeenCalledWith(
      { listId: "list-1", enabled: true, lock: { kind: "random_text", length: 32 } },
      expect.any(Object),
    );
  });

  it("requires matching password fields and permits disabling without re-entering a password", () => {
    render(
      <ProtectForm
        list={list({ scheduled_protection: { lock: { Password: { hash: "stored-hash" } } } })}
        scheduled
        onDone={vi.fn()}
      />,
    );
    const save = screen.getByRole("button", { name: "Save scheduled protection" });
    expect(save).toBeDisabled();
    const inputs = screen.getAllByPlaceholderText(/password/i);
    fireEvent.change(inputs[0], { target: { value: "secret" } });
    fireEvent.change(inputs[1], { target: { value: "different" } });
    expect(save).toBeDisabled();
    fireEvent.change(inputs[1], { target: { value: "secret" } });
    fireEvent.click(save);
    expect(mocks.configure).toHaveBeenLastCalledWith(
      { listId: "list-1", enabled: true, lock: { kind: "password", password: "secret" } },
      expect.any(Object),
    );
    fireEvent.change(inputs[0], { target: { value: "" } });
    fireEvent.click(screen.getByRole("switch", { name: "Protect while schedule is active" }));
    fireEvent.click(save);
    expect(mocks.configure).toHaveBeenLastCalledWith(
      { listId: "list-1", enabled: false, lock: null },
      expect.any(Object),
    );
  });

  it("keeps the existing manual duration action", () => {
    render(<ProtectForm list={list()} onDone={vi.fn()} />);
    expect(screen.queryByRole("switch", { name: "Protect while schedule is active" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Lock for/ }));
    expect(mocks.protect).toHaveBeenCalledWith(
      expect.objectContaining({ minutes: 60, lock: null }),
      expect.any(Object),
    );
    expect(mocks.configure).not.toHaveBeenCalled();
  });
});

describe("scheduled early unlock", () => {
  function show(patch: Partial<BlockList>) {
    mocks.lists.mockReturnValue({ data: [list(patch)], isPending: false, error: null });
    const protection: ProtectionInfo = {
      block_list_id: "list-1",
      block_list_name: "Weekly",
      remaining_seconds: 3600,
      expires_at: "2099-09-21T17:00:00Z",
      prevent_modification: true,
      prevent_service_stop: true,
      prevent_uninstall: true,
    };
    mocks.status.mockReturnValue({ data: [protection] });
    render(
      <TooltipProvider>
        <BlockLists />
      </TooltipProvider>,
    );
  }

  it("uses the scheduled random-text method when there is no manual lock", () => {
    show({ scheduled_protection: { lock: { RandomText: { length: 6 } } } });
    fireEvent.click(screen.getByRole("button", { name: "Unlock Weekly" }));
    expect(mocks.challenge).toHaveBeenCalledWith("list-1");
    fireEvent.change(screen.getByPlaceholderText("Type the string above"), {
      target: { value: "abcdef" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Unlock$/ }));
    expect(mocks.unlock).toHaveBeenCalledWith(
      { listId: "list-1", response: "abcdef" },
      expect.any(Object),
    );
  });

  it("offers no early unlock when None is configured", () => {
    show({ scheduled_protection: { lock: null } });
    expect(screen.getByRole("button", { name: "Unlock Weekly" })).toBeDisabled();
  });

  it("uses a manual password first when the protections overlap", () => {
    show({
      scheduled_protection: { lock: { RandomText: { length: 6 } } },
      lock: { Password: { hash: "hash" } },
      protection: {
        started_at: "2026-09-21T00:00:00Z",
        expires_at: "2099-09-21T17:00:00Z",
        prevent_modification: true,
        prevent_service_stop: true,
        prevent_uninstall: true,
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "Unlock Weekly" }));
    expect(screen.getByPlaceholderText("Enter the password")).toBeInTheDocument();
    expect(mocks.challenge).not.toHaveBeenCalled();
  });
});
