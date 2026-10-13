import type { CSSProperties } from "react";
import type { AnsiColor, TextStyle } from "../bindings";

/**
 * A CSS colour for an ANSI colour. The 16 standard and bright colours follow
 * the theme (tokens.css), so they stay readable in light and dark; the
 * 256-colour cube, the greys and truecolor are exact.
 */
export function cssColor(color: AnsiColor): string {
  if (color.kind === "rgb") return `rgb(${color.red} ${color.green} ${color.blue})`;
  const index = color.index;
  if (index < 16) return `var(--ansi-${index % 8})`;
  if (index < 232) {
    const levels = [0, 95, 135, 175, 215, 255];
    const value = index - 16;
    return `rgb(${levels[Math.floor(value / 36)]} ${levels[Math.floor(value / 6) % 6]} ${levels[value % 6]})`;
  }
  const gray = 8 + (index - 232) * 10;
  return `rgb(${gray} ${gray} ${gray})`;
}

/** Inline styles for a span; `undefined` for plain text, the common case. */
export function spanStyle(style: TextStyle): CSSProperties | undefined {
  const { foreground, background, isBold, isDim, isItalic, isUnderlined, isInverse, isStrikethrough } = style;
  if (!foreground && !background && !isBold && !isDim && !isItalic && !isUnderlined && !isInverse && !isStrikethrough) {
    return undefined;
  }
  let color = foreground ? cssColor(foreground) : undefined;
  let backgroundColor = background ? cssColor(background) : undefined;
  if (isInverse) {
    [color, backgroundColor] = [backgroundColor ?? "var(--surface-log)", color ?? "var(--text)"];
  }
  const decorations = [isUnderlined && "underline", isStrikethrough && "line-through"].filter(Boolean);
  return {
    color,
    backgroundColor,
    fontWeight: isBold ? 600 : undefined,
    fontStyle: isItalic ? "italic" : undefined,
    opacity: isDim ? 0.6 : undefined,
    textDecoration: decorations.length ? decorations.join(" ") : undefined,
  };
}
