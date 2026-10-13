import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSyncExternalStore } from "react";
import { commands, type Settings } from "./bindings";
import { showAlert } from "./ui";

/** Until the app's settings arrive: the defaults. */
let current: Settings = { editorCommand: null, automaticPortMove: true, appearance: "system", logFontSize: 12 };
const listeners = new Set<() => void>();
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

/** Loads the settings and applies them; follows the desktop's style while the theme is System. */
export async function loadSettings() {
  apply(current);
  darkQuery.addEventListener("change", () => apply(current));
  apply(await commands.settings());
}

export function currentSettings(): Settings {
  return current;
}

export function useSettings(): Settings {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}

/** Saves a change, applies it at once, and says so if it couldn't be saved. */
export async function saveSettings(change: Partial<Settings>): Promise<boolean> {
  const next = { ...current, ...change };
  apply(next);
  const result = await commands.saveSettings(next);
  if (result.status === "error") {
    showAlert({ kind: "error", message: result.error.message });
    return false;
  }
  apply(await commands.settings());
  return true;
}

/** The height of one log row at the log font size: the list is virtual, so every row has it. */
export function logRowHeight(settings: Settings): number {
  return Math.round(settings.logFontSize * 1.25);
}

function apply(settings: Settings) {
  current = settings;
  const isDark = settings.appearance === "dark" || (settings.appearance === "system" && darkQuery.matches);
  const root = document.documentElement;
  root.dataset.theme = isDark ? "dark" : "light";
  root.style.setProperty("--log-font-size", `${settings.logFontSize}px`);
  // The window's own title bar follows too; `null` is the desktop's style.
  try {
    void getCurrentWindow()
      .setTheme(settings.appearance === "system" ? null : settings.appearance)
      .catch(() => {});
  } catch {
    // Outside the app's window (tests): nothing to theme.
  }
  listeners.forEach((listener) => listener());
}
