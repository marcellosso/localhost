import { describe, expect, it } from "vitest";
import type { LogLine } from "./bindings";
import {
  failureExcerpt,
  failureTitle,
  hasUrlToOpen,
  portChangeExplanation,
  rowSummary,
  runDuration,
  statusSummary,
} from "./status";
import { project, run, service, status } from "./test/fixtures";

const holder = {
  pid: 123,
  processGroupId: 123,
  name: "python3",
  command: "python3 -m http.server 3000",
  executablePath: null,
  ports: [3000],
  workingDirectory: null,
  owner: { kind: "unmanaged" as const },
  launchdLabel: null,
  kind: "development" as const,
  startTime: { seconds: 0, microseconds: 0 },
};

describe("status summaries", () => {
  it("says where a moved Run listens, and why", () => {
    const web = service("Web", 3000);
    const moved = status(web.id, "running", {
      run: run(web.id, { port: 3001, portChange: { configuredPort: 3000, reason: { kind: "portInUse", holder } } }),
    });
    expect(statusSummary(moved)).toBe("Running · moved to port 3001");
    expect(portChangeExplanation(moved)).toBe("Port 3000 is in use by python3 (PID 123), so this run uses port 3001.");

    const requested = status(web.id, "running", {
      run: run(web.id, { port: 4000, portChange: { configuredPort: 3000, reason: { kind: "requested" } } }),
    });
    expect(statusSummary(requested)).toBe("Running · on port 4000");
    expect(portChangeExplanation(requested)).toBe("Started on port 4000 instead of 3000.");
    expect(portChangeExplanation({ ...requested, state: "stopped" })).toBeNull();
  });

  it("warns about a Port that never opened", () => {
    const web = service("Web", 3000);
    expect(statusSummary(status(web.id, "running", { run: run(web.id, { port: 3000, isPortUnresponsive: true }) }))).toBe(
      "Running · port 3000 not open",
    );
    expect(statusSummary(status(web.id, "starting", { run: run(web.id, { port: 3000 }) }))).toBe(
      "Starting · waiting for port 3000…",
    );
  });

  it("gives the core's words for a failure, else the exit code or signal", () => {
    const web = service("Web");
    const failed = (changes: Parameters<typeof run>[1], failureMessage: string | null = null) =>
      status(web.id, "failed", { run: run(web.id, changes), failureMessage });
    expect(statusSummary(failed({ failure: { kind: "commandNotFound" } }, "Command not found."))).toBe(
      "Failed · Command not found.",
    );
    expect(statusSummary(failed({ termination: { kind: "exited", code: 1 } }))).toBe("Failed · exit 1");
    expect(statusSummary(failed({ termination: { kind: "signaled", signal: 9 } }))).toBe("Failed · SIGKILL");
    expect(statusSummary(failed({ termination: { kind: "signaled", signal: 31 } }))).toBe("Failed · signal 31");
  });

  it("names the Dependencies a Service waits for, Manual ones, and pending restarts", () => {
    const db = service("Database");
    const api = { ...service("API"), dependencies: [db.id] };
    const tools = { ...service("Tools"), isExcludedFromStartAll: true };
    const shop = project("Shop", 0, [db, api, tools]);
    expect(rowSummary(shop, api, status(api.id, "starting", { awaitedDependencies: [db.id] }))).toBe(
      "Waiting for Database…",
    );
    expect(rowSummary(shop, tools, status(tools.id, "stopped"))).toBe("Manual");
    expect(rowSummary(shop, db, status(db.id, "running", { isRestartNeeded: true }))).toBe(
      "Running · restart to apply",
    );
  });

  it("offers Open in Browser only while live with somewhere to go", () => {
    const web = service("Web", 3000);
    const worker = service("Worker");
    expect(hasUrlToOpen(web, status(web.id, "running"))).toBe(true);
    expect(hasUrlToOpen(web, status(web.id, "stopped"))).toBe(false);
    expect(hasUrlToOpen(worker, status(worker.id, "running"))).toBe(false);
    const printed = status(worker.id, "running", { run: run(worker.id, { detectedUrl: "http://localhost:5173" }) });
    expect(hasUrlToOpen(worker, printed)).toBe(true);
  });
});

describe("the failure banner", () => {
  const line = (text: string, channel: LogLine["channel"] = "stdout"): LogLine => ({
    serviceId: null,
    channel,
    spans: [{ text, style: { foreground: null, background: null, isBold: false, isDim: false, isItalic: false, isUnderlined: false, isInverse: false, isStrikethrough: false } }],
  });
  const lines = (count: number) => Array.from({ length: count }, (_, i) => line(`line ${i}`));

  it("shows nothing for short output, which the log shows whole", () => {
    expect(failureExcerpt(lines(30))).toEqual([]);
  });

  it("starts at the first line reading like an error", () => {
    const output = [...lines(35), line("Error: listen EADDRINUSE"), line("    at Server.listen"), ...lines(3)];
    expect(failureExcerpt(output)).toEqual(["Error: listen EADDRINUSE", "    at Server.listen", "line 0", "line 1"]);
  });

  it("else shows the last lines, leaving out localhost's own", () => {
    const output = [...lines(40), line("Exited with code 1", "localhost")];
    expect(failureExcerpt(output)).toEqual(["line 36", "line 37", "line 38", "line 39"]);
  });

  it("titles the failure and says how long the Run lasted", () => {
    const worker = service("Worker");
    const ended = run(worker.id, {
      termination: { kind: "exited", code: 2 },
      endedAt: { secs_since_epoch: 1_001, nanos_since_epoch: 200_000_000 },
    });
    expect(failureTitle(worker, ended, null)).toBe("Worker exited with code 2");
    expect(failureTitle(worker, ended, "Command not found.")).toBe("Command not found.");
    expect(runDuration(ended)).toBe("after 1.2s");
    expect(runDuration({ ...ended, endedAt: { secs_since_epoch: 1_185, nanos_since_epoch: 0 } })).toBe("after 3m 5s");
  });
});
