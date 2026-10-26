import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateCheck } from "./update-check";

const mocks = vi.hoisted(() => ({ authority: vi.fn(), check: vi.fn(), install: vi.fn(), cancel: vi.fn(), installAction: vi.fn(), cancelAction: vi.fn() }));
vi.mock("@/lib/native", () => ({ isTauri: () => true }));
vi.mock("@/lib/updates", () => ({ useAuthorityUpdate: mocks.authority, useUpdate: mocks.check, useInstallUpdate: mocks.install, useCancelAuthorityUpdate: mocks.cancel }));
beforeEach(() => {
  vi.clearAllMocks();
  mocks.check.mockReturnValue({ data: { available: false }, refetch: vi.fn(), isFetching: false });
  mocks.install.mockReturnValue({ mutate: mocks.installAction, isPending: false });
  mocks.cancel.mockReturnValue({ mutate: mocks.cancelAction, isPending: false });
});
function state(phase: string, extras = {}) {
  mocks.authority.mockReturnValue({ data: { phase, code: phase, transaction: "transaction", downloaded: 25, total: 100, ...extras } });
}
describe("authority-owned update status", () => {
  it("shows an active-lock deferral and keeps activation under service control", () => {
    state("deferred", { code: "strict_commitment_active", can_cancel: true });
    render(<UpdateCheck />);
    expect(screen.getByRole("status").textContent).toContain("strict_commitment_active");
    fireEvent.click(screen.getByRole("button", { name: "Activate when permitted" }));
    expect(mocks.installAction).toHaveBeenCalledOnce();
  });
  it("shows durable download progress without offering activation", () => {
    state("downloading", { can_cancel: true });
    render(<UpdateCheck />);
    expect(screen.getByRole("progressbar").getAttribute("value")).toBe("25");
    expect(screen.queryByRole("button", { name: "Activate when permitted" })).toBeNull();
  });
  it("does not offer cancellation or activation during recovery of consumed authorization", () => {
    state("recovery_fault", { can_cancel: false, code: "rollback_incompatible" });
    render(<UpdateCheck />);
    expect(screen.getByRole("status").textContent).toContain("rollback_incompatible");
    expect(screen.queryAllByRole("button")).toHaveLength(0);
  });
  it("can cancel an unconsumed failed stage and retry checking", () => {
    state("recovery_fault", { can_cancel: true, code: "signature_invalid" });
    render(<UpdateCheck />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel staged update" }));
    expect(mocks.cancelAction).toHaveBeenCalledOnce();
  });
});
