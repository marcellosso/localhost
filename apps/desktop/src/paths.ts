import { homeDir } from "@tauri-apps/api/path";

let home: string | null = null;

/** Learns the home folder, for `tildePath`. */
export async function loadHome() {
  try {
    home = (await homeDir()).replace(/\/$/, "");
  } catch {
    // Outside the app (tests): paths stay as they are.
  }
}

/** The path with the home folder as `~`, as macOS abbreviates it. */
export function tildePath(path: string): string {
  if (home && (path === home || path.startsWith(`${home}/`))) return `~${path.slice(home.length)}`;
  return path;
}
