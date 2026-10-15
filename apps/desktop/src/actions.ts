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
import { currentUi, setProgress, setStoppingListener, showAlert } from "./ui";

// What the main window's buttons do. Failures become an alert; a start
// blocked by a taken port becomes the question about it.

export async function start(serviceId: Id, port: number | null = null) {
  askIfBlocked(await commands.startService(serviceId, port));
}

export async function restart(serviceId: Id) {
  askIfBlocked(await commands.restartService(serviceId));
}

export async function stop(serviceId: Id) {
  report(await commands.stopService(serviceId));
}

export async function startAll(projectId: Id) {
  report(await commands.startAll(projectId));
}

export async function stopAll(projectId: Id) {
  report(await commands.stopAll(projectId));
}

/** "Stop it and start": stops whatever holds the blocked start's port, then starts the Service. */
export async function stopHolderAndStart(blocked: BlockedStart) {
  if (!blocked.holder) return;
  askIfBlocked(await commands.stopHolderAndStart(blocked.serviceId, blocked.holder.listener));
}

export async function openUrl(serviceId: Id) {
  report(await commands.openServiceUrl(serviceId));
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

export async function stopListener({ target, listener }: ListenerDetails) {
  setStoppingListener(listener.pid, true);
  report(await commands.stopListener(target, listener));
  setStoppingListener(listener.pid, false);
}

/** What the user must hear about at launch: unreadable Project files, then Orphans. */
export async function runLaunchChecks() {
  const result = await commands.launchChecks();
  if (result.status === "error") return;
  const { unreadableFiles, orphans } = result.data;
  if (unreadableFiles.length > 0) showAlert({ kind: "unreadableFiles", files: unreadableFiles });
  if (orphans.length > 0 && !isAskingAboutOrphans()) showAlert({ kind: "orphans", orphans });
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
