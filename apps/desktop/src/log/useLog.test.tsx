import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { LogBatch, LogSource } from "../bindings";
import { useLog } from "./useLog";

const channels = vi.hoisted(() => [] as { onmessage: (batch: LogBatch) => void }[]);
vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {
    onmessage = () => {};
    constructor() {
      channels.push(this);
    }
  },
}));
vi.mock("../bindings", () => ({
  commands: { openLog: vi.fn().mockResolvedValue({ status: "ok", data: 1 }), closeLog: vi.fn() },
}));

const batch = (count: number): LogBatch => ({
  lines: Array.from({ length: count }, () => ({ serviceId: null, channel: "stdout", spans: [], links: [] })),
  skipped: 0,
  isFinished: false,
});

function Lines({ source }: { source: LogSource }) {
  const buffer = useLog(JSON.stringify(source), source);
  return <p>{buffer.lines.length} lines</p>;
}

describe("useLog", () => {
  it("shows a new stream's first batch, whatever the previous stream reached", async () => {
    const { rerender } = render(<Lines source={{ kind: "run", runId: "a" }} />);
    act(() => channels.at(-1)!.onmessage(batch(3)));
    await screen.findByText("3 lines");

    rerender(<Lines source={{ kind: "run", runId: "b" }} />);
    act(() => channels.at(-1)!.onmessage(batch(2)));
    expect(await screen.findByText("2 lines")).toBeTruthy();
  });
});
