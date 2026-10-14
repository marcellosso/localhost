import { useSyncExternalStore } from "react";
import type { BlockedStart, Id, ImportPreview, ListenerDetails } from "./bindings";

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
  /** Settings; `forEditor` when Open in Editor needs one picked first, then retried. */
  | { kind: "settings"; forEditor?: () => void }
  /** Removing a Project or Service, which stops it first. */
  | { kind: "removeProject"; projectId: Id }
  | { kind: "removeService"; serviceId: Id; dependents: string[] }
  /** A Project file that can't be imported, and why. */
  | { kind: "importFailure"; fileName: string; message: string };

/** An editor over the main window, one at a time. */
export type Sheet =
  | { kind: "addProject" }
  | { kind: "editProject"; projectId: Id }
  | { kind: "addService"; projectId: Id }
  | { kind: "editService"; serviceId: Id }
  | { kind: "detectServices"; projectId: Id }
  | { kind: "environment"; projectId: Id }
  /** The review step of an import, before anything is saved. */
  | { kind: "importProject"; file: string; preview: ImportPreview };

/** The UI's own state, outside the core. */
export type UiState = {
  alerts: readonly Alert[];
  /** Something slow the window waits for, e.g. "Stopping services from the last session…". */
  progress: string | null;
  /** The pids of Listeners being stopped. */
  stoppingListeners: ReadonlySet<number>;
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

export function setStoppingListener(pid: number, isStopping: boolean) {
  const stoppingListeners = new Set(state.stoppingListeners);
  if (isStopping) stoppingListeners.add(pid);
  else stoppingListeners.delete(pid);
  update({ stoppingListeners });
}

/** For tests. */
export function resetUi() {
  state = initialState;
}
