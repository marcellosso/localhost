import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BlockedStart, Listener } from "../bindings";
import { viewFromSnapshot } from "../core";
import { project, service } from "../test/fixtures";
import { resetUi, showAlert } from "../ui";
import { Alerts } from "./Alerts";

const commands = vi.hoisted(() => ({
  startService: vi.fn(),
  stopHolderAndStart: vi.fn(),
  stopOrphans: vi.fn(),
  leaveOrphans: vi.fn(),
}));
vi.mock("../bindings", () => ({ commands }));

const ok = { status: "ok", data: null } as const;
const web = service("Web", 3000);
const view = viewFromSnapshot({ projects: [project("Shop", 0, [web])], statuses: [], projectStates: [] });

const listener: Listener = {
  pid: 123,
  processGroupId: 123,
  name: "python3",
  command: "python3 -m http.server 3000",
  executablePath: null,
  ports: [3000],
  workingDirectory: "/home/me/site",
  owner: { kind: "unmanaged" },
  launchdLabel: null,
  kind: "development",
  startTime: { seconds: 0, microseconds: 0 },
};

function blocked(changes: Partial<BlockedStart> = {}): BlockedStart {
  return {
    serviceId: web.id,
    port: 3000,
    holder: { listener, owner: "Unmanaged", reasonNotStoppable: null },
    nextFreePort: 3001,
    dependents: [],
    ...changes,
  };
}

describe("Alerts", () => {
  beforeEach(() => {
    Object.values(commands).forEach((command) => command.mockResolvedValue(ok));
  });
  afterEach(() => act(resetUi));

  it("asks what to do about a taken port: stop its holder, or start on the next free one", async () => {
    render(<Alerts view={view} />);
    act(() => showAlert({ kind: "blockedStart", blocked: blocked({ dependents: ["Worker"] }) }));

    const dialog = screen.getByRole("alertdialog", { name: "Web can’t start: port 3000 is in use" });
    expect(dialog.textContent).toContain("It is held by python3 -m http.server 3000\nPID 123 · /home/me/site\n(Unmanaged).");
    expect(dialog.textContent).toContain("Starting on 3001: “Worker” depends on it and may still expect port 3000.");
    expect(screen.getByRole("button", { name: "Stop It and Start" })).toBeTruthy();

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Start on 3001" })));
    expect(commands.startService).toHaveBeenCalledWith(web.id, 3001);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("doesn't offer to stop a holder localhost won't stop, and says why", () => {
    render(<Alerts view={view} />);
    const reason = "localhost does not stop itself.";
    act(() =>
      showAlert({
        kind: "blockedStart",
        blocked: blocked({ holder: { listener, owner: "Unmanaged", reasonNotStoppable: reason }, nextFreePort: null }),
      }),
    );
    expect(screen.queryByRole("button", { name: "Stop It and Start" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Start on/ })).toBeNull();
    const text = screen.getByRole("alertdialog").textContent;
    expect(text).toContain(reason);
    expect(text).toContain("No free port was found within 100 ports above it.");
  });

  it("asks one question at a time, Orphans after unreadable files", async () => {
    render(<Alerts view={view} />);
    act(() => {
      showAlert({ kind: "error", message: "The working directory /gone does not exist." });
      showAlert({ kind: "orphans", orphans: ["Shop › Web", "Shop › Worker"] });
    });
    expect(screen.getByRole("alertdialog", { name: "Something went wrong" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    const orphans = screen.getByRole("alertdialog", { name: "2 services from the last session are still running" });
    expect(orphans.textContent).toContain("Shop › Web\nShop › Worker");
    expect(document.activeElement?.textContent).toBe("Stop Them");

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Stop Them" })));
    expect(commands.stopOrphans).toHaveBeenCalled();
  });

  it("cancels with Escape", () => {
    render(<Alerts view={view} />);
    act(() => showAlert({ kind: "orphans", orphans: ["Shop › Web"] }));
    fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Escape" });
    expect(commands.leaveOrphans).toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
