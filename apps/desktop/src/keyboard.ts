import { useSyncExternalStore } from "react";

// Shortcuts use Ctrl on Linux and Windows, and ⌘ on macOS, where the app runs
// for development; Alt stands in for ⌥. Keys are matched by `event.code`, so
// Ctrl+Alt+1 is found whatever character Alt makes the key type.

export const isMac = typeof navigator !== "undefined" && navigator.platform.startsWith("Mac");

/** A shortcut: the primary modifier (Ctrl, or ⌘), plus Shift or Alt, and a key. */
export type Shortcut = {
  /** `KeyboardEvent.code`, e.g. "KeyK", "Digit1", "Period", "BracketLeft". */
  code: string;
  shift?: boolean;
  alt?: boolean;
};

const keyNames: Record<string, string> = { Period: ".", Comma: ",", Slash: "/", BracketLeft: "[", BracketRight: "]" };

/** How the key is written, e.g. "K", "1", ".". */
function keyName(code: string): string {
  return keyNames[code] ?? code.replace(/^(Key|Digit)/, "");
}

/** e.g. "Ctrl+Shift+N" on Linux and Windows, "⇧⌘N" on macOS. */
export function shortcutLabel({ code, shift, alt }: Shortcut): string {
  if (isMac) return `${alt ? "⌥" : ""}${shift ? "⇧" : ""}⌘${keyName(code)}`;
  return `Ctrl+${alt ? "Alt+" : ""}${shift ? "Shift+" : ""}${keyName(code)}`;
}

/** Whether the event is the shortcut, with no other modifier held. */
export function isShortcut(event: KeyboardEvent, { code, shift = false, alt = false }: Shortcut): boolean {
  const primary = isMac ? event.metaKey : event.ctrlKey;
  const other = isMac ? event.ctrlKey : event.metaKey;
  return primary && !other && event.shiftKey === shift && event.altKey === alt && event.code === code;
}

/** Whether the event has the primary modifier (and maybe others): one of the app's shortcuts, not typing. */
export function hasPrimaryModifier(event: KeyboardEvent): boolean {
  return isMac ? event.metaKey : event.ctrlKey;
}

/** Whether keys typed there are text, e.g. a search or a form field. */
export function isTextField(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) return false;
  if (element.isContentEditable || element instanceof HTMLTextAreaElement) return true;
  return (
    element instanceof HTMLInputElement &&
    !["checkbox", "radio", "button", "submit", "range", "color"].includes(element.type)
  );
}

/** The window's shortcuts; the palette shows them too. */
export const SHORTCUTS = {
  palette: { code: "KeyK" },
  ports: { code: "Digit0" },
  newProject: { code: "KeyN" },
  newService: { code: "KeyN", shift: true },
  startOrRestart: { code: "KeyR" },
  stop: { code: "Period" },
  openUrl: { code: "KeyO", shift: true },
  startAll: { code: "KeyR", shift: true },
  stopAll: { code: "Period", shift: true },
  previousService: { code: "BracketLeft" },
  nextService: { code: "BracketRight" },
  settings: { code: "Comma" },
  keyboardShortcuts: { code: "Slash" },
  closeWindow: { code: "KeyW" },
  quit: { code: "KeyQ" },
} satisfies Record<string, Shortcut>;

/** Ctrl+1–9 selects a Project; Ctrl+Alt+1–9 a Service of the selected one. */
export const projectShortcut = (index: number): Shortcut | null => (index < 9 ? { code: `Digit${index + 1}` } : null);
export const serviceShortcut = (index: number): Shortcut | null => (index < 9 ? { code: `Digit${index + 1}`, alt: true } : null);

// MARK: Columns

/** A column of the main window, left to right. */
export type Column = "sidebar" | "list" | "detail";

const columns: Column[] = ["sidebar", "list", "detail"];

/** The column holding `element`, from its `data-column`. */
export function columnOf(element: Element | null): Column | null {
  const column = element?.closest<HTMLElement>("[data-column]")?.dataset.column;
  return columns.includes(column as Column) ? (column as Column) : null;
}

/** Focuses the column's log, else its list, else its first control; false if it has none. */
export function focusColumn(column: Column): boolean {
  const root = document.querySelector<HTMLElement>(`[data-column="${column}"]`);
  const target =
    root?.querySelector<HTMLElement>("[role=log]") ??
    root?.querySelector<HTMLElement>("[role=listbox]") ??
    root?.querySelector<HTMLElement>("button:not(:disabled), input, [tabindex='0']");
  target?.focus();
  return target !== undefined && target !== null && document.activeElement === target;
}

/**
 * Selects the option `offset` rows away in the listbox, stopping at either
 * end, by clicking it as a pointer would. With none selected, steps in from
 * the first or last.
 */
export function stepListbox(listbox: Element, offset: number) {
  const options = Array.from(listbox.querySelectorAll<HTMLElement>("[role=option]"));
  if (options.length === 0) return;
  const current = options.findIndex((option) => option.getAttribute("aria-selected") === "true");
  const next = current < 0 ? (offset > 0 ? 0 : options.length - 1) : Math.min(Math.max(current + offset, 0), options.length - 1);
  if (next === current) return;
  options[next].click();
  options[next].scrollIntoView?.({ block: "nearest" });
}

// MARK: Badges

/** Which rows show their shortcut number: Projects while Ctrl is held, Services while Ctrl+Alt is. */
export type BadgeMode = "none" | "projects" | "services";

let badgeMode: BadgeMode = "none";
const badgeListeners = new Set<() => void>();
let pending: ReturnType<typeof setTimeout> | null = null;

function setBadgeMode(mode: BadgeMode) {
  if (mode === badgeMode) return;
  badgeMode = mode;
  badgeListeners.forEach((listener) => listener());
}

/**
 * Follows the modifier keys: Ctrl alone or Ctrl+Alt held ~0.4 s shows the
 * badges; anything else hides them at once. A key pressed before the delay
 * (e.g. Ctrl+R) cancels it, so quick shortcuts never flash badges.
 */
export function trackBadgeModifiers(event: KeyboardEvent) {
  const isModifierOnly = ["Control", "Meta", "Alt", "Shift"].includes(event.key);
  if (event.type === "keydown" && !isModifierOnly) {
    if (pending) clearTimeout(pending);
    pending = null;
    return;
  }
  const primary = isMac ? event.metaKey : event.ctrlKey;
  const other = isMac ? event.ctrlKey : event.metaKey;
  const target: BadgeMode =
    !primary || other || event.shiftKey ? "none" : event.altKey ? "services" : "projects";
  if (pending) clearTimeout(pending);
  pending = null;
  // Already showing (e.g. Ctrl → Ctrl+Alt): switch at once.
  if (target === "none" || badgeMode !== "none") return setBadgeMode(target);
  pending = setTimeout(() => setBadgeMode(target), 400);
}

/** Hides the badges, e.g. when the window loses focus. */
export function resetBadges() {
  if (pending) clearTimeout(pending);
  pending = null;
  setBadgeMode("none");
}

export function useBadgeMode(): BadgeMode {
  return useSyncExternalStore(
    (listener) => {
      badgeListeners.add(listener);
      return () => badgeListeners.delete(listener);
    },
    () => badgeMode,
  );
}
