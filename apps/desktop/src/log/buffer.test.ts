import { describe, expect, it } from "vitest";
import type { LogLine } from "../bindings";
import { LogBuffer, MAX_LINES } from "./buffer";

const line = (text: string): LogLine => ({ serviceId: null, channel: "stdout", spans: [{ text, style: plain }] });
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
const lines = (from: number, count: number) => Array.from({ length: count }, (_, i) => line(String(from + i)));
const text = (buffer: LogBuffer, index: number) => buffer.lines[index].spans[0].text;

describe("LogBuffer", () => {
  it("appends batches and counts versions", () => {
    const buffer = new LogBuffer();
    buffer.append({ lines: lines(0, 3), skipped: 0, isFinished: false });
    buffer.append({ lines: lines(3, 2), skipped: 0, isFinished: true });
    expect(buffer.lines.map((l) => l.spans[0].text)).toEqual(["0", "1", "2", "3", "4"]);
    expect(buffer.version).toBe(2);
    expect(buffer.isFinished).toBe(true);
  });

  it("trims to the newest lines once 1,000 over the limit", () => {
    const buffer = new LogBuffer();
    buffer.append({ lines: lines(0, MAX_LINES - 1), skipped: 0, isFinished: false });
    buffer.append({ lines: lines(MAX_LINES - 1, 1_000), skipped: 0, isFinished: false });
    expect(buffer.lines.length).toBe(MAX_LINES + 999);
    buffer.append({ lines: lines(MAX_LINES + 999, 2), skipped: 0, isFinished: false });
    expect(buffer.lines.length).toBe(MAX_LINES);
    expect(buffer.dropped).toBe(1_001);
    expect(text(buffer, 0)).toBe("1001");
  });

  it("replaces everything with a burst the core already cut", () => {
    const buffer = new LogBuffer();
    buffer.append({ lines: lines(0, 10), skipped: 0, isFinished: false });
    buffer.append({ lines: lines(100, MAX_LINES), skipped: 90, isFinished: false });
    expect(buffer.lines.length).toBe(MAX_LINES);
    expect(text(buffer, 0)).toBe("100");
    expect(buffer.dropped).toBe(100);
  });
});
