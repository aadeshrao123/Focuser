import { describe, expect, it } from "vitest";
import { formatCountdown, formatDuration } from "./duration";

describe("formatDuration", () => {
  it.each([
    [0, "0s"],
    [45, "45s"],
    [90, "2m"],
    [3600, "1h"],
    [5040, "1h 24m"],
  ])("%is → %s", (seconds, expected) => {
    expect(formatDuration(seconds)).toBe(expected);
  });
});

describe("formatCountdown", () => {
  it.each([
    [0, "0:00"],
    [9, "0:09"],
    [1453, "24:13"],
    [3849, "1:04:09"],
  ])("%is → %s", (seconds, expected) => {
    expect(formatCountdown(seconds)).toBe(expected);
  });

  it("clamps below zero rather than showing a negative clock", () => {
    expect(formatCountdown(-5)).toBe("0:00");
  });
});
