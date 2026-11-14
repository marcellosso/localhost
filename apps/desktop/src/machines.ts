import { Channel } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import { commands, type Id, type Machine, type MachineEvent, type MachineState, type MachineView } from "./bindings";
import { type CoreView, viewFromSnapshot } from "./core";

// Other Machines: computers whose localhost this app shows, reached with
// the system's `ssh`. The core keeps each connected and mirrors its Projects
// and statuses; each is shown through a `CoreView` of its own, as this
// computer's Projects are.

/** One Machine as the window shows it. */
export type ShownMachine = {
  machine: Machine;
  state: MachineState;
  /** How it is, in the core's words: "Connected · localhost 0.5.0", or why not. */
  summary: string;
  isConnected: boolean;
  /** It waits for the user to connect it again. */
  needsUser: boolean;
  /** Why its Projects can't be changed from here now (not connected, or an older localhost there); `null` when they can. */
  editProblem: string | null;
  /** Its Projects and statuses, those of its last connection while it reconnects. */
  view: CoreView;
};

export function shownMachine(view: MachineView): ShownMachine {
  return {
    machine: view.machine,
    state: view.state,
    summary: view.summary,
    isConnected: view.isConnected,
    needsUser: view.needsUser,
    editProblem: view.editProblem,
    view: viewFromSnapshot({ projects: view.projects, statuses: view.statuses, projectStates: view.projectStates }),
  };
}

/** The Machines after `event`. Unchanged Machines keep their identity. */
export function applyMachineEvent(machines: ShownMachine[], event: MachineEvent): ShownMachine[] {
  switch (event.kind) {
    case "machines":
      return event.machines.map(shownMachine);
    case "status":
      return update(machines, event.machineId, (view) => ({
        ...view,
        statuses: new Map(view.statuses).set(event.status.serviceId, event.status),
      }));
    case "projectState":
      return update(machines, event.machineId, (view) => ({
        ...view,
        projectStates: new Map(view.projectStates).set(event.projectState.projectId, event.projectState.state),
      }));
  }
}

function update(machines: ShownMachine[], machineId: Id, change: (view: CoreView) => CoreView): ShownMachine[] {
  return machines.map((shown) => (shown.machine.id === machineId ? { ...shown, view: change(shown.view) } : shown));
}

let current: ShownMachine[] = [];
const listeners = new Set<() => void>();
let frame: number | null = null;
let started = false;

/** Tells React at most once per frame, however many events arrived. */
function publish(next: ShownMachine[]) {
  current = next;
  if (frame !== null) return;
  frame = requestAnimationFrame(() => {
    frame = null;
    listeners.forEach((listener) => listener());
  });
}

/** Subscribes to the Machines once, for the life of the window. */
export async function startMachines() {
  if (started) return;
  started = true;
  // Events can arrive before the snapshot they follow: they wait for it.
  let early: MachineEvent[] | null = [];
  const events = new Channel<MachineEvent>();
  events.onmessage = (event) => {
    if (early) early.push(event);
    else publish(applyMachineEvent(current, event));
  };
  const result = await commands.subscribeMachines(events);
  // Without its list of Machines the app still shows this computer's Projects.
  const snapshot = result.status === "ok" ? result.data.map(shownMachine) : [];
  publish(early.reduce(applyMachineEvent, snapshot));
  early = null;
}

export function currentMachines(): ShownMachine[] {
  return current;
}

export function useMachines(): ShownMachine[] {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}
