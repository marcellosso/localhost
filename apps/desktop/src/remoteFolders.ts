import { useSyncExternalStore } from "react";
import type { MachineRef } from "./editing";

// The remote folder picker, asked for like a system dialog: `chooseOnMachine`
// opens it over whatever is on screen and resolves with the chosen path, or
// `null` on Cancel. One at a time; asking again cancels the open one.

/** What the picker on screen was asked for. */
export type RemotePick = {
  machine: MachineRef;
  /** Where it opens; the home folder there when `null` or not found. */
  start: string | null;
  /** Picks a Project file (YAML) instead of a folder. */
  isFile: boolean;
  resolve: (path: string | null) => void;
};

let current: RemotePick | null = null;
const listeners = new Set<() => void>();

function set(pick: RemotePick | null) {
  current = pick;
  listeners.forEach((listener) => listener());
}

/** A folder (or with `isFile`, a Project file) on the Machine; `null` if cancelled. */
export function chooseOnMachine(
  machine: MachineRef,
  { start = null, isFile = false }: { start?: string | null; isFile?: boolean } = {},
): Promise<string | null> {
  current?.resolve(null);
  return new Promise((resolve) => {
    set({
      machine,
      start,
      isFile,
      resolve: (path) => {
        set(null);
        resolve(path);
      },
    });
  });
}

export function useRemotePick(): RemotePick | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}

/** A Project file, by its extension. */
export function isProjectFile(name: string): boolean {
  return /\.ya?ml$/i.test(name);
}

/** `name` inside the folder `path`, with the folder's own separator. */
export function childPath(path: string, name: string): string {
  const separator = path.includes("\\") && !path.includes("/") ? "\\" : "/";
  return path.endsWith("/") || path.endsWith("\\") ? `${path}${name}` : `${path}${separator}${name}`;
}

/** The folders from the root down to `path`, each with its own path, for a breadcrumb. */
export function pathCrumbs(path: string): { name: string; path: string }[] {
  const windowsRoot = /^[A-Za-z]:[\\/]?/.exec(path);
  const root = windowsRoot ? windowsRoot[0].replace(/[\\/]?$/, "\\") : path.startsWith("/") ? "/" : "";
  const separator = windowsRoot ? "\\" : "/";
  const parts = path.slice(windowsRoot ? windowsRoot[0].length : root.length).split(/[\\/]/).filter(Boolean);
  const crumbs = root ? [{ name: root, path: root }] : [];
  let walked = root;
  for (const part of parts) {
    walked = walked === root ? `${root}${part}` : `${walked}${separator}${part}`;
    crumbs.push({ name: part, path: walked });
  }
  return crumbs;
}
