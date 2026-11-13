import {
  type BlockedStart,
  type CommandError,
  commands,
  type Folder,
  type FolderApp,
  type Id,
  type ListenerDetails,
  type Result,
} from "./bindings";
import { listenerKey } from "./listeners";
import { startOnboarding } from "./onboarding";
import { currentCore } from "./store";
import { currentUi, setProgress, setStoppingListener, showAlert, showSheet } from "./ui";

// What the main window's buttons do, on this computer or, given its id,
// another Machine. Failures become an alert; a start blocked by a taken
// port here becomes the question about it. On another Machine, its row
// says why it Failed.

export async function start(serviceId: Id, port: number | null = null, machineId: Id | null = null) {
  if (machineId) report(await commands.machineStartService(machineId, serviceId));
  else askIfBlocked(await commands.startService(serviceId, port));
}

export async function restart(serviceId: Id, machineId: Id | null = null) {
  if (machineId) report(await commands.machineRestartService(machineId, serviceId));
  else askIfBlocked(await commands.restartService(serviceId));
}

export async function stop(serviceId: Id, machineId: Id | null = null) {
  report(await (machineId ? commands.machineStopService(machineId, serviceId) : commands.stopService(serviceId)));
}

export async function startAll(projectId: Id, machineId: Id | null = null) {
  report(await (machineId ? commands.machineStartAll(machineId, projectId) : commands.startAll(projectId)));
}

export async function stopAll(projectId: Id, machineId: Id | null = null) {
  report(await (machineId ? commands.machineStopAll(machineId, projectId) : commands.stopAll(projectId)));
}

/** "Stop it and start": stops whatever holds the blocked start's port, then starts the Service. */
export async function stopHolderAndStart(blocked: BlockedStart) {
  if (!blocked.holder) return;
  askIfBlocked(await commands.stopHolderAndStart(blocked.serviceId, blocked.holder.listener));
}

/** Open in Browser; another Machine's Port is forwarded to the same port here first. */
export async function openUrl(serviceId: Id, machineId: Id | null = null) {
  report(await (machineId ? commands.machineOpenService(machineId, serviceId) : commands.openServiceUrl(serviceId)));
}

/** Opens the folder in the file manager, a terminal or the editor, asking for the editor first if none is set. */
export async function openFolder(folder: Folder, app: FolderApp) {
  if (app === "editor" && !(await commands.settings()).editorCommand) {
    showAlert({ kind: "settings", forEditor: () => void openFolder(folder, app) });
    return;
  }
  report(await commands.openFolder(folder, app));
}

/** Stops a Run's Listener (its Service) at once; asks before stopping any other. */
export function requestStopListener(details: ListenerDetails) {
  if (details.listener.owner.kind === "run") void stopListener(details);
  else showAlert({ kind: "stopListener", details });
}

export async function stopListener(details: ListenerDetails) {
  const key = listenerKey(details);
  setStoppingListener(key, true);
  report(await commands.stopListener(details.target, details.listener));
  setStoppingListener(key, false);
}

/**
 * What the user must hear about at launch: unreadable Project files, then
 * Orphans. The first launch also opens the welcome, once.
 */
export async function runLaunchChecks() {
  const result = await commands.launchChecks();
  if (result.status === "error") return;
  const { unreadableFiles, orphans } = result.data;
  if (unreadableFiles.length > 0) showAlert({ kind: "unreadableFiles", files: unreadableFiles });
  if (orphans.length > 0 && !isAskingAboutOrphans()) showAlert({ kind: "orphans", orphans });
  const core = currentCore();
  if (core.phase !== "ready") return;
  if ((await startOnboarding(core.view.projects.length)) && !currentUi().sheet) showSheet({ kind: "welcome" });
}

/** A WSL distro found Orphans as it connected: asks about them, unless the question is already up. */
export async function askAboutFoundOrphans() {
  if (isAskingAboutOrphans()) return;
  const result = await commands.launchChecks();
  if (result.status === "ok" && result.data.orphans.length > 0 && !isAskingAboutOrphans()) {
    showAlert({ kind: "orphans", orphans: result.data.orphans });
  }
}

function isAskingAboutOrphans(): boolean {
  return currentUi().alerts.some((alert) => alert.kind === "orphans");
}

export async function stopOrphans() {
  setProgress("Stopping services from the last session…");
  report(await commands.stopOrphans());
  setProgress(null);
}

export async function leaveOrphans() {
  report(await commands.leaveOrphans());
}

export async function dismissUnreadableFiles(reveal: boolean) {
  report(await commands.dismissUnreadableFiles(reveal));
}

export function report(result: Result<unknown, CommandError>) {
  if (result.status === "error") showAlert({ kind: "error", message: result.error.message });
}

function askIfBlocked(result: Result<BlockedStart | null, CommandError>) {
  report(result);
  if (result.status === "ok" && result.data) showAlert({ kind: "blockedStart", blocked: result.data });
}
