import { useSyncExternalStore } from "react";
import { commands, events, type UpdateState } from "./bindings";
import { showAlert } from "./ui";

// The update on offer (updates.rs): the sidebar's notice shows it, Check for
// Updates asks for it now.

let current: UpdateState = { kind: "none" };
const listeners = new Set<() => void>();

function apply(state: UpdateState) {
  current = state;
  listeners.forEach((listener) => listener());
}

/** Follows the update on offer: what it is now, then each change. */
export async function startUpdates() {
  void events.updateState.listen((event) => apply(event.payload));
  apply(await commands.updateState());
}

export function useUpdate(): UpdateState {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}

/** Checks now; the notice shows a newer version, an alert says there is none or why it couldn't check. */
export async function checkForUpdates() {
  const check = await commands.checkForUpdates();
  if (check.kind === "upToDate") {
    showAlert({ kind: "upToDate", version: check.version });
  } else if (check.kind === "failed") {
    showAlert({ kind: "error", message: check.message });
  }
}

/** Opens a release's notes in the browser. */
export async function openReleaseNotes(url: string) {
  const result = await commands.openUrl(url);
  if (result.status === "error") showAlert({ kind: "error", message: result.error.message });
}
