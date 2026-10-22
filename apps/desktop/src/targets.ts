import type { ExecutionTarget, Project, ProjectFolder } from "./bindings";

// Execution Targets as the UI shows them: this machine, or a WSL distro
// (Windows only), where a Project's folder, Services and Listeners are.

export const thisMachine: ExecutionTarget = { kind: "thisMachine" };

const isWindows = typeof navigator !== "undefined" && navigator.platform.startsWith("Win");

/**
 * Whether the target is Windows itself, where a process can't be asked to
 * quit (no signals; Ctrl-Break only reaches localhost's own console): one
 * localhost didn't start is ended at once, and is its own process group.
 */
export function isNativeWindows(target: ExecutionTarget): boolean {
  return isWindows && target.kind === "thisMachine";
}

/** How a command line on the target reads the Port: cmd's `%PORT%` on Windows. */
export function portVariable(target: ExecutionTarget): string {
  return isNativeWindows(target) ? "%PORT%" : "$PORT";
}

/** The Project's folder, on the target it runs on. */
export function projectFolder(project: Project): ProjectFolder {
  return { target: project.executionTarget ?? thisMachine, path: project.path };
}

/** The distro's name; `null` for this machine. */
export function distroOf(target: ExecutionTarget | undefined): string | null {
  return target?.kind === "wsl" ? target.distro : null;
}

/** A string that tells targets apart, e.g. for keys and `<select>` values. */
export function targetId(target: ExecutionTarget): string {
  return target.kind === "wsl" ? `wsl:${target.distro}` : "thisMachine";
}

export function isSameTarget(a: ExecutionTarget, b: ExecutionTarget): boolean {
  return targetId(a) === targetId(b);
}

/** The folder's own name, for Windows paths too. */
export function folderName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? "";
}
