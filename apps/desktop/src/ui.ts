import { useSyncExternalStore } from "react";
import type { BlockedStart, Id, ImportPreview, ListenerDetails } from "./bindings";
import type { MachineRef } from "./editing";

/** A question or report over the main window. They are asked one at a time, in order. */
export type Alert =
  /** A failed action, e.g. a folder that no longer exists. */
  | { kind: "error"; message: string }
  /** A start blocked by a taken port: stop the holder, start elsewhere, or cancel. */
  | { kind: "blockedStart"; blocked: BlockedStart }
  /** Stopping a Listener localhost didn't start. */
  | { kind: "stopListener"; details: ListenerDetails }
  /** At launch: Project files that couldn't be loaded. */
  | { kind: "unreadableFiles"; files: string[] }
  /** At launch: Services of an earlier session still running. */
  | { kind: "orphans"; orphans: string[] }
  /** Settings, on `section` if given; `forEditor` when Open in Editor needs one picked first, then retried. */
  | { kind: "settings"; section?: SettingsSection; forEditor?: () => void }
  /** Every shortcut, grouped (Ctrl+/). */
  | { kind: "keyboardShortcuts" }
  /** Removing a Project or Service, which stops it first; on `machine`, there. */
  | { kind: "removeProject"; projectId: Id; machine?: MachineRef }
  | { kind: "removeService"; serviceId: Id; dependents: string[]; machine?: MachineRef }
  /** A Project file that can't be imported, and why. */
  | { kind: "importFailure"; fileName: string; message: string }
  /** Check for Updates found nothing newer than `version`. */
  | { kind: "upToDate"; version: string };

/** The sections of Settings, in the rail's order. */
export type SettingsSection = "general" | "appearance" | "ports" | "machines" | "agents";

/**
 * An editor over the main window, one at a time. With `machine`, it edits
 * that Machine's Projects, there.
 */
export type Sheet =
  | { kind: "addProject"; machine?: MachineRef }
  | { kind: "editProject"; projectId: Id; machine?: MachineRef }
  | { kind: "addService"; projectId: Id; machine?: MachineRef }
  | { kind: "editService"; serviceId: Id; machine?: MachineRef }
  | { kind: "detectServices"; projectId: Id; machine?: MachineRef }
  | { kind: "environment"; projectId: Id; machine?: MachineRef }
  /** The review step of an import, before anything is saved; `file` is the Machine's with `machine`. */
  | { kind: "importProject"; file: string; preview: ImportPreview; machine?: MachineRef }
  /** Connect your agents: Claude Code, Cursor and Codex, and the command in terminals. */
  | { kind: "connectAgents" }
  /** The first launch's welcome, ending in Add Project…; also Help › Show Welcome…. */
  | { kind: "welcome" };

/** The UI's own state, outside the core. */
export type UiState = {
  alerts: readonly Alert[];
  /** Something slow the window waits for, e.g. "Stopping services from the last session…". */
  progress: string | null;
  /** Listeners being stopped, by `listenerKey`. */
  stoppingListeners: ReadonlySet<string>;
  sheet: Sheet | null;
  /** Whether the command palette (Ctrl+K) is open. */
  isPaletteOpen: boolean;
};

const initialState: UiState = { alerts: [], progress: null, stoppingListeners: new Set(), sheet: null, isPaletteOpen: false };
let state: UiState = initialState;
const listeners = new Set<() => void>();

function update(change: Partial<UiState>) {
  state = { ...state, ...change };
  listeners.forEach((listener) => listener());
}

export function useUi(): UiState {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => state,
  );
}

export function currentUi(): UiState {
  return state;
}

export function showAlert(alert: Alert) {
  update({ alerts: [...state.alerts, alert] });
}

const keys = new WeakMap<Alert, number>();
let nextKey = 0;

/** A key that tells alerts apart, for React. */
export function alertKey(alert: Alert): number {
  let key = keys.get(alert);
  if (key === undefined) keys.set(alert, (key = nextKey++));
  return key;
}

/** Closes the alert on screen, showing the next one. */
export function dismissAlert(alert: Alert) {
  update({ alerts: state.alerts.filter((shown) => shown !== alert) });
}

export function showSheet(sheet: Sheet) {
  update({ sheet });
}

export function closeSheet() {
  update({ sheet: null });
}

export function setPaletteOpen(isPaletteOpen: boolean) {
  update({ isPaletteOpen });
}

export function setProgress(progress: string | null) {
  update({ progress });
}

export function setStoppingListener(key: string, isStopping: boolean) {
  const stoppingListeners = new Set(state.stoppingListeners);
  if (isStopping) stoppingListeners.add(key);
  else stoppingListeners.delete(key);
  update({ stoppingListeners });
}

/** For tests. */
export function resetUi() {
  state = initialState;
}
