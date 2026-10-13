import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { LogLine } from "../bindings";
import { LogBuffer } from "../log/buffer";
import { LogView } from "./LogView";

const plain = {
  foreground: null,
  background: null,
  isBold: false,
  isDim: false,
  isItalic: false,
  isUnderlined: false,
  isInverse: false,
  isStrikethrough: false,
};

describe("LogView", () => {
  it("draws only the rows near the view, not all 10,000", () => {
    const buffer = new LogBuffer();
    const lines: LogLine[] = Array.from({ length: 10_000 }, (_, i) => ({
      serviceId: null,
      channel: i % 2 ? "stderr" : "stdout",
      spans: [{ text: `line ${i}`, style: plain }],
    }));
    buffer.append({ lines, skipped: 0, isFinished: false });
    const { container } = render(<LogView buffer={buffer} label="Web output" emptyText="" />);
    const rows = container.querySelectorAll("[role=log] > div > div");
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThanOrEqual(60);
  });

  it("prefixes combined lines with their Service", () => {
    const buffer = new LogBuffer();
    buffer.append({
      lines: [{ serviceId: "a", channel: "stdout", spans: [{ text: "ready", style: plain }] }],
      skipped: 0,
      isFinished: false,
    });
    render(<LogView buffer={buffer} prefixes={new Map([["a", { text: "api", color: "red" }]])} label="All" emptyText="" />);
    expect(screen.getByText("api")).toBeTruthy();
    expect(screen.getByText("ready")).toBeTruthy();
  });

  it("says when there is no output", () => {
    render(<LogView buffer={new LogBuffer()} label="Web output" emptyText="Web has no output yet." />);
    expect(screen.getByRole("log").textContent).toBe("Web has no output yet.");
  });
});
