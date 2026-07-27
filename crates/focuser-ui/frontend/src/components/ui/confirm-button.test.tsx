import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfirmButton } from "./confirm-button";

describe("ConfirmButton", () => {
  it("does not fire on the first click", async () => {
    const onConfirm = vi.fn();
    render(<ConfirmButton onConfirm={onConfirm}>Clear</ConfirmButton>);

    await userEvent.click(screen.getByRole("button"));

    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByRole("button")).toHaveTextContent("Click again to confirm");
  });

  it("fires on the second click", async () => {
    const onConfirm = vi.fn();
    render(<ConfirmButton onConfirm={onConfirm}>Clear</ConfirmButton>);

    const button = screen.getByRole("button");
    await userEvent.click(button);
    await userEvent.click(button);

    expect(onConfirm).toHaveBeenCalledOnce();
    expect(button).toHaveTextContent("Clear");
  });

  it("disarms when it loses focus", async () => {
    render(<ConfirmButton onConfirm={vi.fn()}>Clear</ConfirmButton>);

    await userEvent.click(screen.getByRole("button"));
    await userEvent.tab();

    expect(screen.getByRole("button")).toHaveTextContent("Clear");
  });

  it("accepts a custom confirm label", async () => {
    render(
      <ConfirmButton onConfirm={vi.fn()} confirmLabel="Really?">
        Clear
      </ConfirmButton>,
    );

    await userEvent.click(screen.getByRole("button"));

    expect(screen.getByRole("button")).toHaveTextContent("Really?");
  });
});
