import { describe, expect, it } from "vitest";
import { applyMachineEvent, shownMachine } from "./machines";
import { machineView, project, service, status } from "./test/fixtures";

describe("Machines", () => {
  const web = service("Web", 3000);
  const shop = project("Shop", 0, [web]);

  it("shows each Machine through a view of its own", () => {
    const devbox = shownMachine(machineView("devbox", [shop], [status(web.id, "running")]));
    expect(devbox.view.projects).toEqual([shop]);
    expect(devbox.view.statuses.get(web.id)?.state).toBe("running");
  });

  it("follows a Machine's statuses, leaving the others as they were", () => {
    const machines = [shownMachine(machineView("devbox", [shop])), shownMachine(machineView("lab"))];
    const devboxId = machines[0].machine.id;

    const after = applyMachineEvent(machines, { kind: "status", machineId: devboxId, status: status(web.id, "failed") });
    expect(after[0].view.statuses.get(web.id)?.state).toBe("failed");
    expect(after[1]).toBe(machines[1]);

    const states = applyMachineEvent(after, {
      kind: "projectState",
      machineId: devboxId,
      projectState: { projectId: shop.id, state: "failed" },
    });
    expect(states[0].view.projectStates.get(shop.id)).toBe("failed");
  });

  it("takes every Machine afresh when the list or a Machine's state changes", () => {
    const lost = machineView("devbox", [shop], [], {
      state: { kind: "unavailable", detail: { problem: { kind: "lost" }, isRetrying: true } },
      summary: "The connection to devbox was lost. Trying again…",
      isConnected: false,
    });
    const after = applyMachineEvent([], { kind: "machines", machines: [lost] });
    expect(after.map((shown) => [shown.machine.name, shown.isConnected, shown.summary])).toEqual([
      ["devbox", false, "The connection to devbox was lost. Trying again…"],
    ]);
  });
});
