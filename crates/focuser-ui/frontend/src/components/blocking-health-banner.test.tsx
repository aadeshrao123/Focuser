import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BlockingHealth } from "@/bindings";
import { BlockingHealthBanner } from "./blocking-health-banner";

const useBlockingHealth = vi.hoisted(() => vi.fn());
const useBrowserStatus = vi.hoisted(() => vi.fn());
vi.mock("@/lib/commands", () => ({ useBlockingHealth, useBrowserStatus }));

function onSystem(agent: string) {
  vi.spyOn(navigator, "userAgent", "get").mockReturnValue(agent);
}

function health(patch: Partial<BlockingHealth>) {
  useBlockingHealth.mockReturnValue({
    data: { active_lists: 1, extension_connected: false, hosts_writable: false, ...patch },
  });
}

const banner = () => screen.queryByRole("alert");

describe("BlockingHealthBanner", () => {
  beforeEach(() => useBlockingHealth.mockReset());
  afterEach(() => vi.restoreAllMocks());

  it("warns when rules are on but nothing can block websites", () => {
    health({});
    render(<BlockingHealthBanner />);
    expect(banner()).toHaveTextContent("Websites are not blocked");
  });

  it("suggests administrator rights on Windows only", () => {
    // On Linux that would mean running a desktop app as root (#60).
    health({});
    onSystem("Mozilla/5.0 (X11; Linux x86_64)");
    const { unmount } = render(<BlockingHealthBanner />);
    expect(banner()).not.toHaveTextContent("administrator");
    unmount();

    onSystem("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
    render(<BlockingHealthBanner />);
    expect(banner()).toHaveTextContent("administrator");
  });

  it("opens the browser list to install the extension from", () => {
    health({});
    useBrowserStatus.mockReturnValue({
      isPending: false,
      data: [
        {
          browser: "Firefox",
          display_name: "Firefox",
          running: true,
          extension_connected: false,
          incognito_allowed: false,
          store_url: "https://addons.mozilla.org/",
          launch_name: "firefox",
        },
      ],
    });
    render(<BlockingHealthBanner />);
    fireEvent.click(screen.getByRole("button", { name: /get the extension/i }));
    expect(screen.getByRole("dialog")).toHaveTextContent("Firefox");
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
