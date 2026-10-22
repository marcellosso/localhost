import { describe, expect, it } from "vitest";
import type { LogLine, TextStyle } from "../bindings";
import { lineMatches, segments } from "./segments";

const plain: TextStyle = {
  foreground: null,
  background: null,
  isBold: false,
  isDim: false,
  isItalic: false,
  isUnderlined: false,
  isInverse: false,
  isStrikethrough: false,
};
const cyan: TextStyle = { ...plain, foreground: { kind: "indexed", index: 6 } };

const line: LogLine = {
  serviceId: null,
  channel: "stdout",
  spans: [
    { text: "Local: ", style: plain },
    { text: "http://localhost:3000/", style: cyan },
  ],
  links: [{ start: 7, end: 29, url: "http://localhost:3000/" }],
};

describe("log segments", () => {
  it("cuts at styles, links and filter matches", () => {
    expect(segments(line, "host").map(({ text, url, isMatch }) => [text, url !== null, isMatch])).toEqual([
      ["Local: ", false, false],
      ["http://local", true, false],
      ["host", true, true],
      [":3000/", true, false],
    ]);
    expect(segments(line, "")[1].style).toBe(cyan);
  });

  it("matches the text or the Service's prefix, ignoring case", () => {
    expect(lineMatches(line, "LOCAL")).toBe(true);
    expect(lineMatches(line, "api")).toBe(false);
    expect(lineMatches(line, "api", "api   ")).toBe(true);
  });
});
