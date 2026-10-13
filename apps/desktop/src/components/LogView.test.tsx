import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { LogLine } from "../bindings";
import { LogBuffer } from "../log/buffer";
import { LogView } from "./LogView";

const commands = vi.hoisted(() => ({ openUrl: vi.fn() }));
vi.mock("../bindings", () => ({ commands }));

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
      links: [],
    }));
    buffer.append({ lines, skipped: 0, isFinished: false });
    const { container } = render(<LogView buffer={buffer} label="Web output" rowHeight={18} />);
    const rows = container.querySelectorAll("[role=log] > div > div");
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThanOrEqual(60);
  });

  it("prefixes combined lines with their Service", () => {
    const buffer = new LogBuffer();
    buffer.append({
      lines: [{ serviceId: "a", channel: "stdout", spans: [{ text: "ready", style: plain }], links: [] }],
      skipped: 0,
      isFinished: false,
    });
    render(<LogView buffer={buffer} prefixes={new Map([["a", { text: "api", color: "red" }]])} label="All" rowHeight={18} />);
    expect(screen.getByText("api")).toBeTruthy();
    expect(screen.getByText("ready")).toBeTruthy();
  });

  it("shows only lines matching the filter, and opens the local URLs they print", () => {
    const buffer = new LogBuffer();
    const line = (text: string, links: LogLine["links"] = []): LogLine => ({
      serviceId: null,
      channel: "stdout",
      spans: [{ text, style: plain }],
      links,
    });
    buffer.append({
      lines: [line("compiling"), line("Local: http://localhost:5173/", [{ start: 7, end: 29, url: "http://localhost:5173/" }])],
      skipped: 0,
      isFinished: false,
    });
    const { container } = render(<LogView buffer={buffer} label="Web" filter="LOCAL" rowHeight={18} />);
    expect(screen.queryByText("compiling")).toBeNull();
    expect([...container.querySelectorAll("[class*=match]")].map((match) => match.textContent)).toEqual(["Local", "local"]);

    fireEvent.click(screen.getByRole("link", { name: "host:5173/" }));
    expect(commands.openUrl).toHaveBeenCalledWith("http://localhost:5173/");
  });
});
