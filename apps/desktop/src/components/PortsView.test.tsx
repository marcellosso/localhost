import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Listener, ListenerDetails } from "../bindings";
import { viewFromSnapshot } from "../core";
import { project, service } from "../test/fixtures";
import { currentUi, resetUi } from "../ui";
import { PortsView } from "./PortsView";

const commands = vi.hoisted(() => ({ listeners: vi.fn(), stopListener: vi.fn() }));
vi.mock("../bindings", () => ({ commands }));

const web = service("Web", 3000);
const shop = project("Shop", 0, [web]);
const view = viewFromSnapshot({ projects: [shop], statuses: [], projectStates: [] });

function details(name: string, port: number, changes: Partial<Listener>, owner: string): ListenerDetails {
  const listener: Listener = {
    pid: port,
    processGroupId: port,
    name,
    command: `/usr/bin/${name} --port ${port}`,
    executablePath: `/usr/bin/${name}`,
    ports: [port],
    workingDirectory: "/home/me",
    owner: { kind: "unmanaged" },
    launchdLabel: null,
    kind: "development",
    startTime: { seconds: 0, microseconds: 0 },
    ...changes,
  };
  return { listener, owner, reasonNotStoppable: null };
}

const runs = details("node", 3000, { owner: { kind: "run", serviceId: web.id, runId: "r" } }, "Shop › Web");
const stray = details("python3", 8000, {}, "Unmanaged");
const app = details("Spotify", 57621, { kind: "app" }, "App");

describe("PortsView", () => {
  afterEach(() => act(resetUi));

  async function renderPorts() {
    commands.listeners.mockResolvedValue({ status: "ok", data: [runs, stray, app] });
    commands.stopListener.mockResolvedValue({ status: "ok", data: null });
    render(<PortsView view={view} />);
    await act(async () => {});
  }

  it("lists development servers, names a Service's by its Service, and folds apps away", async () => {
    await renderPorts();
    const rows = screen.getAllByRole("option");
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual([
      "Shop › Web, port 3000, node --port 3000",
      "python3, port 8000, python3 --port 8000",
    ]);

    fireEvent.click(screen.getByRole("button", { name: "Apps & System (1)" }));
    expect(screen.getAllByRole("option")).toHaveLength(3);
  });

  it("shows the selected Listener's process", async () => {
    await renderPorts();
    fireEvent.click(screen.getAllByRole("option")[1]);
    const detail = screen.getByRole("region", { name: "python3 process" });
    expect(detail.textContent).toContain("/usr/bin/python3 --port 8000");
    expect(detail.textContent).toContain("localhost didn’t start this process");
  });

  it("stops a Service's Listener at once, and asks before stopping any other", async () => {
    await renderPorts();
    const [serviceStop, strayStop] = screen.getAllByRole("button", { name: /^Stop / });

    await act(async () => fireEvent.click(serviceStop));
    expect(commands.stopListener).toHaveBeenCalledWith(runs.listener);
    expect(currentUi().alerts).toEqual([]);

    fireEvent.click(strayStop);
    expect(currentUi().alerts).toEqual([{ kind: "stopListener", details: stray }]);
  });
});
