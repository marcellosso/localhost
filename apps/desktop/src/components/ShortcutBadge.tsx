import { projectShortcut, serviceShortcut, shortcutLabel, useBadgeMode } from "../keyboard";

/**
 * A row's shortcut, e.g. "Ctrl+2", over its trailing edge while Ctrl (for
 * Projects) or Ctrl+Alt (for Services) is held. Rows past 9 have none.
 * Visual only: the palette and the buttons' help name the same shortcuts.
 */
export function ShortcutBadge({ kind, index }: { kind: "projects" | "services"; index: number }) {
  const mode = useBadgeMode();
  const shortcut = kind === "projects" ? projectShortcut(index) : serviceShortcut(index);
  if (mode !== kind || !shortcut) return null;
  return (
    <span className="shortcutBadge" aria-hidden="true">
      {shortcutLabel(shortcut)}
    </span>
  );
}
