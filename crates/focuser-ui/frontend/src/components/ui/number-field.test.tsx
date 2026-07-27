import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { NumberField } from "./number-field";

/** The steppers are aria-hidden, so reach them the way a pointer would. */
const steppers = (container: HTMLElement) => {
  const [up, down] = container.querySelectorAll("button");
  return { up, down };
};

describe("NumberField", () => {
  it("commits on blur, not on every keystroke", async () => {
    const user = userEvent.setup();
    const onCommit = vi.fn();
    render(<NumberField aria-label="Minutes" value={5} onCommit={onCommit} min={1} max={90} />);

    const input = screen.getByLabelText("Minutes");
    await user.clear(input);
    await user.type(input, "30");
    expect(onCommit).not.toHaveBeenCalled();

    await user.tab();
    expect(onCommit).toHaveBeenCalledExactlyOnceWith(30);
  });

  it("snaps back when the typed number is out of range", async () => {
    const user = userEvent.setup();
    const onCommit = vi.fn();
    render(<NumberField aria-label="Minutes" value={5} onCommit={onCommit} min={1} max={90} />);

    const input = screen.getByLabelText("Minutes");
    await user.clear(input);
    await user.type(input, "500");
    await user.tab();

    expect(onCommit).not.toHaveBeenCalled();
    expect(input).toHaveValue(5);
  });

  // The bug this guards: callers save through a mutation, so `value` keeps
  // reporting the old number until the write lands. Presses inside that window
  // used to read the stale prop — a burst of three all computed the same
  // result, and the late-arriving old value then reset the field on top.
  it("accumulates presses that land before the value prop catches up", async () => {
    const commits: number[] = [];
    let land: (() => void) | undefined;

    function Host() {
      const [value, setValue] = useState(25);
      return (
        <NumberField
          aria-label="Minutes"
          value={value}
          step={5}
          min={1}
          max={90}
          onCommit={(next) => {
            commits.push(next);
            // Held until the test releases it, standing in for the round trip.
            land = () => setValue(next);
          }}
        />
      );
    }

    const { container } = render(<Host />);
    const { up } = steppers(container);

    // Synchronous, so every press happens while the prop still says 25.
    fireEvent.click(up);
    fireEvent.click(up);
    fireEvent.click(up);

    expect(commits).toEqual([30, 35, 40]);
    expect(screen.getByLabelText("Minutes")).toHaveValue(40);

    // The final write lands; the field must not jump anywhere.
    act(() => land?.());
    expect(screen.getByLabelText("Minutes")).toHaveValue(40);
  });

  it("clamps at the bounds instead of committing past them", async () => {
    const user = userEvent.setup();
    const onCommit = vi.fn();
    const { container } = render(
      <NumberField aria-label="Minutes" value={3} onCommit={onCommit} min={1} max={5} />,
    );

    const { up, down } = steppers(container);
    await user.click(down);
    await user.click(down);
    await user.click(down);
    await user.click(up);

    expect(onCommit.mock.calls.flat()).toEqual([2, 1, 2]);
  });

  it("follows the value when it changes from outside", async () => {
    function Host() {
      const [value, setValue] = useState(10);
      return (
        <>
          <NumberField aria-label="Minutes" value={value} onCommit={setValue} />
          <button type="button" onClick={() => setValue(42)}>
            Load
          </button>
        </>
      );
    }

    const user = userEvent.setup();
    render(<Host />);
    await user.click(screen.getByRole("button", { name: "Load" }));

    expect(screen.getByLabelText("Minutes")).toHaveValue(42);
  });
});
