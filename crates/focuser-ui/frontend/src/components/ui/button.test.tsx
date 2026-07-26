import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Button } from "./button";

describe("Button", () => {
  it("defaults to type=button so it cannot accidentally submit a form", () => {
    render(<Button>Save</Button>);
    expect(screen.getByRole("button", { name: "Save" })).toHaveProperty("type", "button");
  });

  it("applies tone on top of variant rather than replacing it", () => {
    render(
      <Button variant="ghost" tone="destructive">
        Delete
      </Button>,
    );
    const cls = screen.getByRole("button", { name: "Delete" }).className;

    expect(cls).toContain("bg-transparent");
    expect(cls).toContain("text-destructive");
  });

  it("lets a call-site className win over the variant's", () => {
    render(<Button className="rounded-none">Square</Button>);
    expect(screen.getByRole("button", { name: "Square" }).className).toContain("rounded-none");
  });
});
