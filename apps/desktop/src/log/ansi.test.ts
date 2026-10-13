import { describe, expect, it } from "vitest";
import type { TextStyle } from "../bindings";
import { cssColor, spanStyle } from "./ansi";

const style = (overrides: Partial<TextStyle>): TextStyle => ({
  foreground: null,
  background: null,
  isBold: false,
  isDim: false,
  isItalic: false,
  isUnderlined: false,
  isInverse: false,
  isStrikethrough: false,
  ...overrides,
});

describe("ANSI colours", () => {
  it("maps the standard and bright colours to theme colours", () => {
    expect(cssColor({ kind: "indexed", index: 1 })).toBe("var(--ansi-1)");
    expect(cssColor({ kind: "indexed", index: 9 })).toBe("var(--ansi-1)");
  });

  it("keeps the 256-colour cube, the greys and truecolor exact", () => {
    expect(cssColor({ kind: "indexed", index: 196 })).toBe("rgb(255 0 0)");
    expect(cssColor({ kind: "indexed", index: 232 })).toBe("rgb(8 8 8)");
    expect(cssColor({ kind: "rgb", red: 1, green: 2, blue: 3 })).toBe("rgb(1 2 3)");
  });

  it("styles nothing for plain text", () => {
    expect(spanStyle(style({}))).toBeUndefined();
  });

  it("swaps colours for inverse and combines decorations", () => {
    const css = spanStyle(
      style({ foreground: { kind: "indexed", index: 2 }, isInverse: true, isUnderlined: true, isStrikethrough: true }),
    );
    expect(css?.color).toBe("var(--surface-log)");
    expect(css?.backgroundColor).toBe("var(--ansi-2)");
    expect(css?.textDecoration).toBe("underline line-through");
  });
});
