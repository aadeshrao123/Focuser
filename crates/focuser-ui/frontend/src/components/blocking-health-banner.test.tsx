import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BlockingHealth } from "@/bindings";
import { BlockingHealthBanner } from "./blocking-health-banner";

const useBlockingHealth = vi.hoisted(() => vi.fn());
const useAuthoritySession = vi.hoisted(() => vi.fn());
vi.mock("@/lib/commands", () => ({ useBlockingHealth, useAuthoritySession }));

function health(patch: Partial<BlockingHealth>) {
  useBlockingHealth.mockReturnValue({
    data: { active_lists: 1, extension_connected: false, hosts_writable: false, ...patch },
  });
}

const banner = () => screen.queryByRole("alert");

describe("BlockingHealthBanner", () => {
  beforeEach(() => {
    useBlockingHealth.mockReset();
    useAuthoritySession.mockReturnValue({ data: "connected" });
  });

  it("shows disconnection even with cached healthy state", () => {
    useBlockingHealth.mockReturnValue({
      isError: true,
      data: { active_lists: 0, hosts_writable: true },
    });
    render(<BlockingHealthBanner />);
    expect(banner()).toHaveTextContent("Focuser authority disconnected");
    expect(banner()).toHaveTextContent("No replacement blocking engine");
  });

  it("shows missing session coverage while the service is available", () => {
    health({ hosts_writable: true });
    useAuthoritySession.mockReturnValue({ data: "missing" });
    render(<BlockingHealthBanner />);
    expect(banner()).toHaveTextContent("Desktop coverage is degraded");
  });

  it("warns when rules are on but nothing can enforce them", () => {
    health({});
    render(<BlockingHealthBanner />);
    expect(banner()).toHaveTextContent("Blocking is not in force");
  });

  it("stays quiet when the extension is enforcing", () => {
    health({ extension_connected: true });
    render(<BlockingHealthBanner />);
    expect(banner()).toBeNull();
  });

  it("stays quiet when the hosts file is writable", () => {
    health({ hosts_writable: true });
    render(<BlockingHealthBanner />);
    expect(banner()).toBeNull();
  });

  it("stays quiet when nothing is meant to be blocked", () => {
    // No active lists means no broken promise, so a warning here would just
    // train the user to ignore the banner.
    health({ active_lists: 0 });
    render(<BlockingHealthBanner />);
    expect(banner()).toBeNull();
  });

  it("renders nothing before the first response arrives", () => {
    useBlockingHealth.mockReturnValue({ data: undefined });
    render(<BlockingHealthBanner />);
    expect(banner()).toBeNull();
  });
});
