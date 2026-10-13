import type { Listener } from "./bindings";

/** e.g. "python3 -m http.server 3000\nPID 123 · /home/me/site" */
export function processDescription(listener: Listener): string {
  const folder = listener.workingDirectory ? ` · ${listener.workingDirectory}` : "";
  return `${listener.command}\nPID ${listener.pid}${folder}`;
}

/** "port 3000" or "ports 3000, 3001". */
export function portsText(listener: Listener): string {
  return `${listener.ports.length === 1 ? "port" : "ports"} ${listener.ports.join(", ")}`;
}

/**
 * The command with its executable shortened to a name, e.g.
 * "python3 -m http.server 5199", else the working directory.
 */
export function listenerSubtitle(listener: Listener): string {
  const command = listener.command.trim();
  if (!command) return listener.workingDirectory ?? "";
  const executable = listener.executablePath;
  if (executable && command.startsWith(executable)) return baseName(executable) + command.slice(executable.length);
  const [first, ...rest] = command.split(" ");
  if (!first.startsWith("/")) return command;
  return [baseName(first), ...rest].join(" ");
}

function baseName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}
