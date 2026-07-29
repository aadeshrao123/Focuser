import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdatePill } from "./update-pill";

const useUpdate = vi.hoisted(() => vi.fn());
vi.mock("@/lib/updates", () => ({ useUpdate }));

function show() {
  render(
    <MemoryRouter>
      <UpdatePill />
    </MemoryRouter>,
  );
  return screen.queryByRole("link");
}

describe("UpdatePill", () => {
  beforeEach(() => useUpdate.mockReset());

  it("says nothing when the app is current", () => {
    useUpdate.mockReturnValue({ data: { available: false } });
    expect(show()).toBeNull();
  });

  it("says nothing when the check failed", () => {
    // No network is the common case. A sidebar that reports it every launch
    // teaches people to stop reading the sidebar.
    useUpdate.mockReturnValue({ data: undefined, error: new Error("offline") });
    expect(show()).toBeNull();
  });

  it("names the version when there is one", () => {
    useUpdate.mockReturnValue({ data: { available: true, version: "0.8.0" } });
    expect(show()).toHaveTextContent("0.8.0");
  });

  it("still shows up when the updater gave no version", () => {
    useUpdate.mockReturnValue({ data: { available: true } });
    expect(show()).toHaveTextContent("Update available");
  });

  it("links to the row in Settings that installs it", () => {
    useUpdate.mockReturnValue({ data: { available: true, version: "0.8.0" } });
    // Contains, not equals: the app runs on a HashRouter, so the real href is
    // prefixed with `#`.
    expect(show()?.getAttribute("href")).toContain("/settings?highlight=updates");
  });
});
