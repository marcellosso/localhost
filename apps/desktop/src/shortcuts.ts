import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useRef } from "react";
import { openUrl, restart, start, startAll, stop, stopAll } from "./actions";
import { commands, type Project } from "./bindings";
import { type CoreView, serviceStatus } from "./core";
import {
  columnOf,
  focusColumn,
  hasPrimaryModifier,
  isShortcut,
  isTextField,
  resetBadges,
  SHORTCUTS,
  stepListbox,
  trackBadgeModifiers,
} from "./keyboard";
import type { Navigator } from "./palette";
import type { ServiceSelection } from "./selection";
import { isActive } from "./status";
import { currentUi, setPaletteOpen, showAlert, showSheet } from "./ui";

export type ShortcutContext = {
  view: CoreView;
  /** The selected Project; none while Ports is shown. */
  selected: Project | undefined;
  selection: ServiceSelection;
  setSelection: (selection: ServiceSelection) => void;
  navigator: Navigator;
};

/**
 * The main window's keyboard, after the macOS menus and ColumnNavigation:
 *
 * - Ctrl+K the palette, Ctrl+0 Ports, Ctrl+1–9 Projects, Ctrl+Alt+1–9 the
 *   selected Project's Services, Ctrl+[ / Ctrl+] the previous / next row.
 * - Ctrl+R starts (or restarts) the selected Service, Ctrl+. stops it,
 *   Ctrl+Shift+O opens its URL; Ctrl+Shift+R / Ctrl+Shift+. Start / Stop All.
 * - Ctrl+N New Project, Ctrl+Shift+N New Service, Ctrl+, Settings.
 * - Ctrl+W closes the window (Services keep running), Ctrl+Q quits; these
 *   work under sheets and alerts too.
 * - Across the columns (sidebar → Services → log): → (and ↩ in the sidebar)
 *   moves right, ← (and Escape in the log) moves left; ↑/↓ move through a list.
 *
 * Nothing else happens under a sheet or an alert, and keys typed into fields are left alone.
 */
export function useWindowShortcuts(context: ShortcutContext) {
  const latest = useRef(context);
  latest.current = context;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      trackBadgeModifiers(event);
      if (event.defaultPrevented) return;
      if (isShortcut(event, SHORTCUTS.quit) || isShortcut(event, SHORTCUTS.closeWindow)) {
        event.preventDefault();
        void (isShortcut(event, SHORTCUTS.quit) ? commands.quitApp() : getCurrentWindow().close());
        return;
      }
      const ui = currentUi();
      if (ui.sheet || ui.alerts.length > 0) return;
      if (ui.isPaletteOpen) {
        if (isShortcut(event, SHORTCUTS.palette)) {
          event.preventDefault();
          setPaletteOpen(false);
        }
        return;
      }
      const handled = hasPrimaryModifier(event) ? runShortcut(event, latest.current) : navigate(event);
      if (handled) event.preventDefault();
    };
    const onKeyUp = (event: KeyboardEvent) => trackBadgeModifiers(event);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", resetBadges);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", resetBadges);
    };
  }, []);
}

/** Runs the shortcut the event is, if any. */
function runShortcut(event: KeyboardEvent, { view, selected, selection, setSelection, navigator }: ShortcutContext): boolean {
  const is = (shortcut: Parameters<typeof isShortcut>[1]) => isShortcut(event, shortcut);
  const service =
    selection.kind === "service" ? selected?.services.find((service) => service.id === selection.serviceId) : undefined;
  const state = service ? serviceStatus(view, service.id).state : null;
  const isLive = state === "starting" || state === "running";

  if (is(SHORTCUTS.palette)) setPaletteOpen(true);
  else if (is(SHORTCUTS.ports)) navigator.selectPorts();
  else if (is(SHORTCUTS.settings)) showAlert({ kind: "settings" });
  else if (is(SHORTCUTS.newProject)) showSheet({ kind: "addProject" });
  else if (is(SHORTCUTS.newService)) {
    if (selected) showSheet({ kind: "addService", projectId: selected.id });
  } else if (is(SHORTCUTS.startOrRestart)) {
    if (service && state !== "stopping") void (isLive ? restart(service.id) : start(service.id));
  } else if (is(SHORTCUTS.stop)) {
    if (service && isLive) void stop(service.id);
  } else if (is(SHORTCUTS.openUrl)) {
    if (service && isLive && serviceStatus(view, service.id).urlToOpen) void openUrl(service.id);
  } else if (is(SHORTCUTS.startAll)) {
    if (selected && selected.services.length > 0) void startAll(selected.id);
  } else if (is(SHORTCUTS.stopAll)) {
    if (selected?.services.some((service) => isActive(serviceStatus(view, service.id).state))) void stopAll(selected.id);
  } else if (is(SHORTCUTS.previousService) || is(SHORTCUTS.nextService)) {
    if (selected) setSelection(adjacentRow(selected, selection, is(SHORTCUTS.nextService) ? 1 : -1));
  } else {
    const digit = /^Digit([1-9])$/.exec(event.code);
    if (!digit || event.shiftKey) return false;
    const index = Number(digit[1]) - 1;
    if (event.altKey) {
      const target = selected?.services[index];
      if (selected && target) navigator.selectService(selected.id, target.id);
    } else {
      const target = view.projects[index];
      if (target) navigator.selectProject(target.id);
    }
  }
  // A shortcut of the app's, even when there was nothing to do: not the webview's (Ctrl+R reloads).
  return true;
}

/**
 * The row `offset` rows away in the Project's list ("All services", then its
 * Services), stopping at either end.
 */
export function adjacentRow(project: Project, selection: ServiceSelection, offset: number): ServiceSelection {
  const rows: ServiceSelection[] = [
    { kind: "all" },
    ...project.services.map((service) => ({ kind: "service", serviceId: service.id }) as const),
  ];
  const current = rows.findIndex((row) =>
    row.kind === "all" ? selection.kind === "all" : selection.kind === "service" && selection.serviceId === row.serviceId,
  );
  return rows[Math.min(Math.max((current < 0 ? 0 : current) + offset, 0), rows.length - 1)];
}

/** Arrow keys, ↩ and Escape across and within the columns. */
function navigate(event: KeyboardEvent): boolean {
  if (event.altKey || event.shiftKey || event.metaKey || event.ctrlKey) return false;
  const focused = document.activeElement;
  if (isTextField(focused) || focused?.closest("[role=menu], [role=dialog]")) return false;
  const column = columnOf(focused);
  if (!column) return false;
  const listbox = focused?.closest("[role=listbox]");

  switch (event.key) {
    case "ArrowUp":
    case "ArrowDown":
      if (!listbox) return false;
      stepListbox(listbox, event.key === "ArrowDown" ? 1 : -1);
      return true;
    case "ArrowRight":
      return column === "sidebar" ? focusColumn("list") : column === "list" ? focusColumn("detail") : false;
    case "Enter":
      return column === "sidebar" && focused === listbox ? focusColumn("list") : false;
    case "ArrowLeft":
      return column === "list" ? focusColumn("sidebar") : column === "detail" ? focusColumn("list") : false;
    case "Escape":
      return column === "detail" ? focusColumn("list") : false;
    default:
      return false;
  }
}
