import { afterEach, describe, expect, it, vi } from "vitest";
import { isShortcut, resetBadges, shortcutLabel, stepListbox, trackBadgeModifiers, useBadgeMode } from "./keyboard";
import { renderHook } from "@testing-library/react";

const key = (init: KeyboardEventInit & { type?: string }) => new KeyboardEvent(init.type ?? "keydown", init);

describe("keyboard", () => {
  afterEach(() => {
    resetBadges();
    vi.useRealTimers();
  });

  it("writes shortcuts the Linux way", () => {
    expect(shortcutLabel({ code: "KeyK" })).toBe("Ctrl+K");
    expect(shortcutLabel({ code: "KeyN", shift: true })).toBe("Ctrl+Shift+N");
    expect(shortcutLabel({ code: "Digit2", alt: true })).toBe("Ctrl+Alt+2");
    expect(shortcutLabel({ code: "Period", shift: true })).toBe("Ctrl+Shift+.");
  });

  it("matches a shortcut by its key's code, with exactly its modifiers", () => {
    expect(isShortcut(key({ code: "Digit1", key: "¡", ctrlKey: true, altKey: true }), { code: "Digit1", alt: true })).toBe(true);
    expect(isShortcut(key({ code: "KeyR", key: "R", ctrlKey: true, shiftKey: true }), { code: "KeyR" })).toBe(false);
    expect(isShortcut(key({ code: "KeyR", key: "r" }), { code: "KeyR" })).toBe(false);
  });

  it("steps through a listbox by choosing the next option, stopping at the ends", () => {
    document.body.innerHTML = `<ul role="listbox">
      <li role="option" aria-selected="false">A</li><li role="option" aria-selected="true">B</li><li role="option" aria-selected="false">C</li>
    </ul>`;
    const listbox = document.querySelector("[role=listbox]")!;
    const options = Array.from(document.querySelectorAll<HTMLElement>("[role=option]"));
    const clicked = vi.fn();
    options.forEach((option) => option.addEventListener("click", () => clicked(option.textContent)));

    stepListbox(listbox, 1);
    expect(clicked).toHaveBeenLastCalledWith("C");
    options[1].setAttribute("aria-selected", "false");
    options[2].setAttribute("aria-selected", "true");
    stepListbox(listbox, 1);
    expect(clicked).toHaveBeenCalledTimes(1);
  });

  it("shows badges after Ctrl is held a moment, but not for a quick shortcut", () => {
    vi.useFakeTimers();
    const { result, rerender } = renderHook(() => useBadgeMode());

    trackBadgeModifiers(key({ key: "Control", ctrlKey: true }));
    trackBadgeModifiers(key({ key: "r", code: "KeyR", ctrlKey: true }));
    vi.advanceTimersByTime(500);
    rerender();
    expect(result.current).toBe("none");

    trackBadgeModifiers(key({ key: "Control", ctrlKey: true }));
    vi.advanceTimersByTime(500);
    rerender();
    expect(result.current).toBe("projects");
    // Ctrl → Ctrl+Alt switches at once.
    trackBadgeModifiers(key({ key: "Alt", ctrlKey: true, altKey: true }));
    rerender();
    expect(result.current).toBe("services");
    trackBadgeModifiers(key({ type: "keyup", key: "Control" }));
    rerender();
    expect(result.current).toBe("none");
  });
});
