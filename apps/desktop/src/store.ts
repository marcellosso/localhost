import { Channel } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import { commands, type CoreEvent } from "./bindings";
import { applyEvent, type CoreView, viewFromSnapshot } from "./core";

export type CoreStore =
  | { phase: "loading" }
  /** The core couldn't open its data, or the subscription failed. */
  | { phase: "failed"; message: string }
  | { phase: "ready"; view: CoreView };

let current: CoreStore = { phase: "loading" };
const listeners = new Set<() => void>();
let frame: number | null = null;

/** Tells React at most once per frame, however many events arrived. */
function publish(next: CoreStore) {
  current = next;
  if (frame !== null) return;
  frame = requestAnimationFrame(() => {
    frame = null;
    listeners.forEach((listener) => listener());
  });
}

let started = false;

/** Subscribes to the core once, for the life of the window. */
export async function startCore() {
  if (started) return;
  started = true;
  // Events can arrive before the snapshot they follow: they wait for it.
  let early: CoreEvent[] | null = [];
  const events = new Channel<CoreEvent>();
  events.onmessage = (event) => {
    if (early) early.push(event);
    else if (current.phase === "ready") publish({ phase: "ready", view: applyEvent(current.view, event) });
  };
  const result = await commands.subscribe(events);
  if (result.status === "error") {
    publish({ phase: "failed", message: result.error.message });
    return;
  }
  publish({ phase: "ready", view: early.reduce(applyEvent, viewFromSnapshot(result.data)) });
  early = null;
}

export function currentCore(): CoreStore {
  return current;
}

export function useCore(): CoreStore {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}
