import type { LogLine, Project, Run, Service, ServiceState, StatusView, SystemTime, Termination } from "./bindings";

// The words a Service's status shows, as in the macOS app (StateIndicator.swift,
// ProjectDetailView.swift, LogPane.swift). Errors come worded by the core.

/** Starting, Running or Stopping: a process may be alive. */
export function isActive(state: ServiceState): boolean {
  return state === "starting" || state === "running" || state === "stopping";
}

/** Running although its Port has not accepted connections in time. */
export function hasPortWarning(status: StatusView): boolean {
  return status.state === "running" && status.run?.isPortUnresponsive === true;
}

/**
 * Why the active Run is not on the Service's Port, e.g. "Port 3000 is in use
 * by python3 (PID 123), so this run uses port 3001."
 */
export function portChangeExplanation(status: StatusView): string | null {
  const run = status.run;
  if (!isActive(status.state) || run?.port == null || !run.portChange) return null;
  const { configuredPort, reason } = run.portChange;
  if (reason.kind === "requested") return `Started on port ${run.port} instead of ${configuredPort}.`;
  if (reason.holder) {
    return `Port ${configuredPort} is in use by ${reason.holder.name} (PID ${reason.holder.pid}), so this run uses port ${run.port}.`;
  }
  return `Port ${configuredPort} is in use, so this run uses port ${run.port}.`;
}

/** e.g. "Running", "Running · moved to port 3001", "Failed · exit 1", "Failed · Command not found." */
export function statusSummary(status: StatusView): string {
  const run = status.run;
  switch (status.state) {
    case "stopped":
      return "Stopped";
    case "starting":
      if (run?.port != null && run.endedAt === null && status.awaitedDependencies.length === 0) {
        return `Starting · waiting for port ${run.port}…`;
      }
      return "Starting…";
    case "running":
      if (hasPortWarning(status) && run?.port != null) return `Running · port ${run.port} not open`;
      if (run?.port != null && run.portChange) {
        return run.portChange.reason.kind === "portInUse" ? `Running · moved to port ${run.port}` : `Running · on port ${run.port}`;
      }
      return "Running";
    case "stopping":
      return "Stopping…";
    case "exited":
      return "Exited · code 0";
    case "failed":
      if (status.failureMessage) return `Failed · ${status.failureMessage}`;
      if (run?.termination?.kind === "exited") return `Failed · exit ${run.termination.code}`;
      if (run?.termination?.kind === "signaled") return `Failed · ${signalName(run.termination.signal)}`;
      return "Failed";
  }
}

/** What a Service row says: "Manual", "Waiting for API…", the summary, and "restart to apply". */
export function rowSummary(project: Project, service: Service, status: StatusView): string {
  if (isManual(service, status)) return "Manual";
  if (status.isRestartNeeded) return `${statusSummary(status)} · restart to apply`;
  if (status.state === "starting" && status.awaitedDependencies.length > 0) {
    const names = status.awaitedDependencies.flatMap((id) => project.services.find((s) => s.id === id)?.name ?? []);
    if (names.length > 0) return `Waiting for ${listFormat.format(names)}…`;
  }
  return statusSummary(status);
}

/** The row summary's tooltip: why it says what it says. */
export function rowSummaryHelp(service: Service, status: StatusView): string {
  if (isManual(service, status)) return "Start All skips this service. Start it on its own.";
  if (status.isRestartNeeded) return "Its configuration changed since it started. Restart it to apply.";
  const run = status.run;
  return (
    portChangeExplanation(status) ??
    status.failureMessage ??
    (run?.termination ? terminationSummary(run.termination) : null) ??
    statusSummary(status)
  );
}

/** Stopped and left out of Start All. */
export function isManual(service: Service, status: StatusView): boolean {
  return service.isExcludedFromStartAll === true && status.state === "stopped";
}

/** Whether the summary should draw attention without being a failure. */
export function isWarning(status: StatusView): boolean {
  return hasPortWarning(status) || portChangeExplanation(status) !== null || status.isRestartNeeded;
}

/** "Exited with code 1", "Terminated by signal SIGTERM". */
export function terminationSummary(termination: Termination): string {
  return termination.kind === "exited"
    ? `Exited with code ${termination.code}`
    : `Terminated by signal ${signalName(termination.signal)}`;
}

/** The failure banner's title, e.g. "Worker exited with code 1", or the typed reason. */
export function failureTitle(service: Service, run: Run, failureMessage: string | null): string {
  if (failureMessage) return failureMessage;
  if (run.termination?.kind === "exited") return `${service.name} exited with code ${run.termination.code}`;
  if (run.termination?.kind === "signaled") return `${service.name} was ended by ${signalName(run.termination.signal)}`;
  return `${service.name} failed`;
}

/** How long a Run that ended ran, e.g. "after 1.2s", "after 3m 5s". */
export function runDuration(run: Run): string | null {
  // A Run that never launched (e.g. a port conflict) didn't run at all.
  if (!run.endedAt || !run.termination) return null;
  const seconds = Math.max(0, toSeconds(run.endedAt) - toSeconds(run.startedAt));
  if (seconds < 60) return `after ${seconds.toFixed(1)}s`;
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = Math.floor(seconds % 60);
  return hours > 0 ? `after ${hours}h ${minutes}m` : `after ${minutes}m ${rest}s`;
}

/** With no more output lines than this, the log shows the error already. */
export const SHORT_RUN_LINE_COUNT = 30;
const EXCERPT_LINE_COUNT = 4;
const ERROR_PATTERN = /\b(error|exception|fatal|panic|traceback|failed)\b/i;

/**
 * The failure banner's excerpt of a Failed Run's output: in its last 40
 * output lines, the first that reads like an error and the few after it,
 * else the last few. Nothing for short output, which the log shows whole.
 */
export function failureExcerpt(lines: readonly LogLine[]): string[] {
  const output: string[] = [];
  for (let index = lines.length - 1; index >= 0 && output.length < 40; index--) {
    if (lines[index].channel !== "localhost") output.push(lineText(lines[index]));
  }
  output.reverse();
  if (output.length <= SHORT_RUN_LINE_COUNT) return [];
  const start = output.findIndex((text) => ERROR_PATTERN.test(text));
  return start >= 0 ? output.slice(start, start + EXCERPT_LINE_COUNT) : output.slice(-EXCERPT_LINE_COUNT);
}

export function lineText(line: LogLine): string {
  return line.spans.map((span) => span.text).join("");
}

const listFormat = new Intl.ListFormat("en", { type: "conjunction" });

/** "A", "A and B", "A, B, and C". */
export function formatList(items: string[]): string {
  return listFormat.format(items);
}

/** Signal numbers shared by Linux and macOS; others are shown as numbers. */
const SIGNALS: Record<number, string> = {
  1: "SIGHUP",
  2: "SIGINT",
  3: "SIGQUIT",
  4: "SIGILL",
  5: "SIGTRAP",
  6: "SIGABRT",
  8: "SIGFPE",
  9: "SIGKILL",
  11: "SIGSEGV",
  13: "SIGPIPE",
  14: "SIGALRM",
  15: "SIGTERM",
};

function signalName(signal: number): string {
  return SIGNALS[signal] ?? `signal ${signal}`;
}

function toSeconds(time: SystemTime): number {
  return time.secs_since_epoch + time.nanos_since_epoch / 1e9;
}
