import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RunStats } from "../bindings";
import { viewFromSnapshot } from "../core";
import { project, run, service, status } from "../test/fixtures";
import {
  HISTORY_LENGTH,
  ProjectStats,
  STATS_INTERVAL,
  ServiceStatsButton,
  appendSample,
  formatCpu,
  formatMemory,
} from "./Stats";

const commands = vi.hoisted(() => ({ runStats: vi.fn() }));
vi.mock("../bindings", () => ({ commands }));

function stats(changes: Partial<RunStats> = {}): RunStats {
  return {
    runId: "run-1",
    processCount: 3,
    memoryBytes: 84 * 1024 * 1024,
    cpuTimeMicroseconds: 1_000_000,
    sampledAtMicroseconds: 5_000_000,
    cpuPercent: null,
    ...changes,
  };
}

describe("appendSample", () => {
  it("keeps the last minute of one Run and its peak memory", () => {
    let history = appendSample(undefined, stats({ memoryBytes: 300 }));
    for (let index = 0; index < HISTORY_LENGTH + 5; index++) history = appendSample(history, stats({ memoryBytes: 100 }));
    expect(history?.samples).toHaveLength(HISTORY_LENGTH);
    expect(history?.peakMemory).toBe(300);
  });

  it("starts over for another Run and drops the history without one", () => {
    const history = appendSample(undefined, stats({ memoryBytes: 300 }));
    const next = appendSample(history, stats({ runId: "run-2", memoryBytes: 100 }));
    expect(next).toEqual({ samples: [stats({ runId: "run-2", memoryBytes: 100 })], peakMemory: 100 });
    expect(appendSample(next, null)).toBeUndefined();
  });
});

describe("formatting", () => {
  it("shows CPU with a decimal only when small, and memory in units of 1024", () => {
    expect([formatCpu(null), formatCpu(2.345), formatCpu(153.6)]).toEqual(["–", "2.3%", "154%"]);
    expect([formatMemory(512), formatMemory(1536), formatMemory(84.24 * 1024 * 1024), formatMemory(1.3 * 1024 ** 3)]).toEqual([
      "512 bytes",
      "1.5 KB",
      "84.2 MB",
      "1.3 GB",
    ]);
  });
});

describe("stats popovers", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    commands.runStats.mockReset();
  });

  const tick = () => act(async () => vi.advanceTimersByTimeAsync(STATS_INTERVAL));

  it("samples a Service only while its popover is open, passing the previous sample", async () => {
    const web = service("Web");
    const first = stats();
    const second = stats({ cpuPercent: 42.4, sampledAtMicroseconds: 6_500_000 });
    commands.runStats.mockResolvedValueOnce({ status: "ok", data: first }).mockResolvedValue({ status: "ok", data: second });
    render(<ServiceStatsButton service={web} index={0} />);
    expect(commands.runStats).not.toHaveBeenCalled();

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Show CPU and Memory of Web" })));
    expect(commands.runStats).toHaveBeenLastCalledWith(web.id, null);
    expect(screen.getByRole("group", { name: "CPU –" })).toBeTruthy();
    expect(screen.getByRole("group", { name: "Memory 84.0 MB" })).toBeTruthy();

    await tick();
    expect(commands.runStats).toHaveBeenLastCalledWith(web.id, first);
    expect(screen.getByRole("group", { name: "CPU 42%" })).toBeTruthy();
    expect(screen.getByText("3")).toBeTruthy();

    fireEvent.keyDown(screen.getByRole("dialog", { name: "CPU and memory of Web" }), { key: "Escape" });
    const calls = commands.runStats.mock.calls.length;
    await tick();
    await tick();
    expect(commands.runStats).toHaveBeenCalledTimes(calls);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("says when the Service is no longer running", async () => {
    commands.runStats.mockResolvedValue({ status: "ok", data: null });
    render(<ServiceStatsButton service={service("Web")} index={0} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Show CPU and Memory of Web" })));
    expect(screen.getByText("Measuring…")).toBeTruthy();
    await tick();
    expect(screen.getByText("Not running.")).toBeTruthy();
  });

  it("lists a Project's active Services with their total", async () => {
    const [web, api, docs] = [service("Web"), service("API"), service("Docs")];
    const shop = project("Shop", 0, [web, api, docs]);
    const view = viewFromSnapshot({
      projects: [shop],
      statuses: [status(web.id, "running", { run: run(web.id) }), status(api.id, "running", { run: run(api.id) })],
      projectStates: [],
    });
    commands.runStats.mockImplementation(async (serviceId: string) => ({
      status: "ok",
      data: stats({ runId: serviceId, cpuPercent: serviceId === web.id ? 10 : 5, memoryBytes: 1024 * 1024 }),
    }));
    render(<ProjectStats project={shop} view={view} onClose={() => {}} />);
    await act(async () => {});

    expect(commands.runStats.mock.calls.map(([serviceId]) => serviceId).sort()).toEqual([web.id, api.id].sort());
    const rows = screen.getAllByRole("row").map((row) => row.textContent);
    expect(rows).toEqual(["ServiceCPUMemory", "Web10%1.0 MB", "API5.0%1.0 MB", "Total15%2.0 MB"]);
  });
});
