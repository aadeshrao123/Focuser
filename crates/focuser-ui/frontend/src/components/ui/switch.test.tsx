import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Switch } from "./switch";

describe("Switch", () => {
  it("exposes its state through aria-checked", () => {
    render(<Switch checked onCheckedChange={vi.fn()} aria-label="Notifications" />);

    expect(screen.getByRole("switch", { name: "Notifications" })).toBeChecked();
  });

  it("reports the flipped value", async () => {
    const onCheckedChange = vi.fn();
    render(<Switch checked={false} onCheckedChange={onCheckedChange} aria-label="Notifications" />);

    await userEvent.click(screen.getByRole("switch"));

    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("does not fire while disabled", async () => {
    const onCheckedChange = vi.fn();
    render(
      <Switch checked onCheckedChange={onCheckedChange} disabled aria-label="Notifications" />,
    );

    await userEvent.click(screen.getByRole("switch"));

    expect(onCheckedChange).not.toHaveBeenCalled();
  });
});
