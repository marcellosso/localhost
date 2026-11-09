import { useSyncExternalStore } from "react";
import { commands } from "./bindings";

/** The page that says what localhost collects and how to turn it off; the core links the same one. */
export const PRIVACY_URL = "https://www.getlocalhost.app/privacy";

/** What the app sends when it opens, worded once for the notice and Settings. */
export const TELEMETRY_SUMMARY =
  "When localhost opens it sends a random id, its version, your OS and architecture, and how it was installed. Never your projects, commands, paths or logs.";

let isNoticePending = false;
const listeners = new Set<() => void>();

function setNoticePending(pending: boolean) {
  isNoticePending = pending;
  listeners.forEach((listener) => listener());
}

/** At launch: whether the sidebar should say, once, what is sent. */
export async function loadTelemetryNotice() {
  setNoticePending(await commands.telemetryNoticePending());
}

/** The notice was read: it never shows again. */
export function acknowledgeTelemetryNotice() {
  setNoticePending(false);
  void commands.acknowledgeTelemetryNotice();
}

export function useTelemetryNoticePending(): boolean {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => isNoticePending,
  );
}
