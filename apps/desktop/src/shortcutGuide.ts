import { isMac, projectShortcut, SHORTCUTS, type Shortcut, serviceShortcut, shortcutLabel } from "./keyboard";

// The Keyboard Shortcuts dialog's content (Ctrl+/), after the macOS app's
// KeyboardShortcutsView.swift: what this window's keyboard does, grouped.

export type GuideEntry = {
  title: string;
  /** Each drawn as a key cap, e.g. ["Ctrl+[", "Ctrl+]"]. */
  keys: string[];
};

export type GuideGroup = { title: string; entries: GuideEntry[] };

const entry = (title: string, ...shortcuts: (Shortcut | string)[]): GuideEntry => ({
  title,
  keys: shortcuts.map((shortcut) => (typeof shortcut === "string" ? shortcut : shortcutLabel(shortcut))),
});

/** e.g. "Ctrl+1–9". */
const range = (first: Shortcut) => `${shortcutLabel(first)}–9`;

/** Ctrl+L clears the log on macOS too, as ⌃L does in the macOS app. */
export const CLEAR_LOG = isMac ? "⌃L" : "Ctrl+L";

/** The modifiers that show the rows' number badges while held. */
export const BADGE_MODIFIERS = isMac ? ["⌘", "⌥⌘"] : ["Ctrl", "Ctrl+Alt"];

/** Two columns of groups. */
export function shortcutGuide(): GuideGroup[][] {
  return [
    [
      {
        title: "Go",
        entries: [
          entry("Command Palette", SHORTCUTS.palette),
          entry("Project 1–9", range(projectShortcut(0)!)),
          entry("Service 1–9 of the project", range(serviceShortcut(0)!)),
          entry("Previous / Next Service", SHORTCUTS.previousService, SHORTCUTS.nextService),
          entry("Ports", SHORTCUTS.ports),
        ],
      },
      {
        title: "Service",
        entries: [
          entry("Start or Restart", SHORTCUTS.startOrRestart),
          entry("Stop", SHORTCUTS.stop),
          entry("Open URL", SHORTCUTS.openUrl),
          entry("New Service", SHORTCUTS.newService),
        ],
      },
      {
        title: "Project",
        entries: [
          entry("Start All", SHORTCUTS.startAll),
          entry("Stop All", SHORTCUTS.stopAll),
          entry("New Project", SHORTCUTS.newProject),
        ],
      },
    ],
    [
      {
        title: "Log",
        entries: [
          entry("Filter", { code: "KeyF" }),
          entry("Clear", CLEAR_LOG),
          entry("Scroll a line", "↑", "↓"),
          entry("Scroll a page", "Page Up", "Page Down"),
          entry("Top / Bottom", "Home", "End"),
        ],
      },
      {
        title: "Columns",
        entries: [
          entry("Next / Previous column", "→", "←"),
          entry("Move in a list", "↑", "↓"),
          entry("From the log to the services", "Esc"),
        ],
      },
      {
        title: "App",
        entries: [
          entry("Settings", SHORTCUTS.settings),
          entry("Keyboard Shortcuts", SHORTCUTS.keyboardShortcuts),
          entry("Close Window", SHORTCUTS.closeWindow),
          entry("Quit", SHORTCUTS.quit),
        ],
      },
    ],
  ];
}
