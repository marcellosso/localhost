import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DirectoryListing, Project, ServiceSuggestion } from "../bindings";
import { shownMachine, type ShownMachine } from "../machines";
import { childPath, pathCrumbs } from "../remoteFolders";
import { machineView, project, service } from "../test/fixtures";
import { currentUi, resetUi, showAlert, showSheet } from "../ui";
import { Alerts } from "./Alerts";
import { machinePlace, projectMenuItems } from "./ServicesColumn";
import { Sheets } from "./Sheets";
import { machineAddItems } from "./Sidebar";

// Another Machine's Projects, edited from here: the same sheets, whose
// calls go to the Machine, and its folders picked with the remote picker.

const commands = vi.hoisted(() => ({
  machineListDirectory: vi.fn(),
  machineScanFolder: vi.fn(),
  machineSuggestedProjectKey: vi.fn(),
  machineAddProject: vi.fn(),
  machineRemoveService: vi.fn(),
  machineServiceRelations: vi.fn(),
  machineSuggestedServiceKey: vi.fn(),
  machineDetectPythonEnvironment: vi.fn(),
  machineEditService: vi.fn(),
  relativeFolder: vi.fn(),
  commandLineUsesPort: vi.fn(),
  keyFromName: vi.fn(),
  wslDistros: vi.fn(),
  chooseFolder: vi.fn(),
}));
vi.mock("../bindings", () => ({ commands }));

const machines = vi.hoisted(() => ({ shown: [] as ShownMachine[] }));
vi.mock("../machines", async (actual) => ({
  ...(await actual<typeof import("../machines")>()),
  useMachines: () => machines.shown,
  currentMachines: () => machines.shown,
}));

const ok = <T,>(data: T) => ({ status: "ok", data }) as const;
const failure = (message: string) => ({ status: "error", error: { error: { kind: "unsupported" }, message } }) as const;

const api = service("API", 4000);
const shop: Project = { ...project("Shop", 0, [api]), path: "/srv/shop" };
const devbox = shownMachine(machineView("devbox", [shop]));
const machine = { id: devbox.machine.id, name: "devbox" };

function listing(path: string, names: string[]): DirectoryListing {
  return {
    path,
    parent: path === "/" ? null : path.replace(/\/[^/]+$/, "") || "/",
    home: "/home/me",
    entries: names.map((name) => ({ name, isDirectory: !name.endsWith(".yaml") })),
  };
}

function suggestion(name: string): ServiceSuggestion {
  return {
    name,
    key: name.toLowerCase(),
    workingDirectory: ".",
    commandLine: `pnpm ${name.toLowerCase()}`,
    port: null,
    portCanChange: true,
    isRecommended: true,
    foundIn: "package.json",
    detail: name.toLowerCase(),
  };
}

function renderSheets() {
  const selected = { project: vi.fn(), service: vi.fn() };
  render(
    <>
      <Sheets view={devbox.view} onSelectProject={selected.project} onSelectService={selected.service} />
      <Alerts view={devbox.view} />
    </>,
  );
  return selected;
}

describe("editing another Machine's Projects", () => {
  beforeEach(() => {
    machines.shown = [devbox];
    Object.values(commands).forEach((command) => command.mockResolvedValue(ok(null)));
    commands.machineSuggestedProjectKey.mockImplementation(async (_: string, name: string) => ok(name.toLowerCase()));
    commands.machineServiceRelations.mockResolvedValue(ok({ cycles: [], dependents: [] }));
    commands.commandLineUsesPort.mockResolvedValue(true);
    commands.wslDistros.mockResolvedValue(["Ubuntu"]);
  });
  afterEach(() => act(resetUi));

  it("adds a Project from a folder picked and scanned on the Machine", async () => {
    commands.machineListDirectory.mockImplementation(async (_: string, path: string | null) =>
      ok(path === "/home/me/shop" ? listing("/home/me/shop", ["web"]) : listing("/home/me", ["shop", "notes"])),
    );
    commands.machineScanFolder.mockResolvedValue(ok([suggestion("Dev")]));
    commands.machineAddProject.mockResolvedValue(ok({ ...shop, id: "new" }));
    const selected = renderSheets();
    act(() => showSheet({ kind: "addProject", machine }));

    expect(screen.getByText("New Project on devbox")).toBeTruthy();
    // A Machine's Projects never run in a distro of this computer.
    expect(commands.wslDistros).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Runs in")).toBeNull();

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Folder" })));
    expect(await screen.findByText("Choose a Folder on devbox")).toBeTruthy();
    expect(commands.machineListDirectory).toHaveBeenCalledWith(machine.id, null, false, false);
    await act(async () => fireEvent.doubleClick(await screen.findByRole("option", { name: "shop, folder" })));
    expect(commands.machineListDirectory).toHaveBeenLastCalledWith(machine.id, "/home/me/shop", false, false);
    await screen.findByRole("option", { name: "web, folder" });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Choose" })));

    expect(screen.queryByText("Choose a Folder on devbox")).toBeNull();
    expect(commands.machineScanFolder).toHaveBeenCalledWith(machine.id, "/home/me/shop");
    await waitFor(() => expect((screen.getByLabelText("Key") as HTMLInputElement).value).toBe("shop"));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Add Project" })));
    expect(commands.machineAddProject).toHaveBeenCalledWith(machine.id, "shop", "shop", "/home/me/shop", [
      expect.objectContaining({ name: "Dev", commandLine: "pnpm dev" }),
    ]);
    expect(currentUi().sheet).toBeNull();
    expect(selected.project).toHaveBeenCalledWith("new", machine);
  });

  it("says why a folder of the Machine can't be opened, and goes to a typed path", async () => {
    commands.machineListDirectory.mockImplementation(async (_: string, path: string | null) =>
      path === "/root" ? failure("/root can’t be opened: Permission denied.") : ok(listing("/home/me", ["shop"])),
    );
    renderSheets();
    act(() => showSheet({ kind: "addProject", machine }));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Folder" })));
    const path = (await screen.findByLabelText("Path on devbox")) as HTMLInputElement;
    await waitFor(() => expect(path.value).toBe("/home/me"));

    fireEvent.change(path, { target: { value: "/root" } });
    await act(async () => fireEvent.keyDown(path, { key: "Enter" }));
    expect(screen.getByRole("alert").textContent).toBe("/root can’t be opened: Permission denied.");
    // Still where it was, and Cancel leaves the folder unchosen.
    await act(async () => fireEvent.click(screen.getAllByRole("button", { name: "Cancel" }).at(-1)!));
    expect(screen.queryByText("Choose a Folder on devbox")).toBeNull();
    expect(commands.machineScanFolder).not.toHaveBeenCalled();
  });

  it("edits a Service there, naming the Machine, and removes one after asking", async () => {
    commands.machineSuggestedServiceKey.mockResolvedValue(ok("api"));
    renderSheets();
    act(() => showSheet({ kind: "editService", serviceId: api.id, machine }));
    expect(screen.getByText(/on devbox ·/)).toBeTruthy();
    expect(screen.getByText("/srv/shop")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(commands.machineEditService).toHaveBeenCalledWith(machine.id, api.id, expect.objectContaining({ name: "API" }));

    act(() => showAlert({ kind: "removeService", serviceId: api.id, dependents: [], machine }));
    expect(screen.getByText("Remove “API”?")).toBeTruthy();
    expect(screen.getByText(/stopped first, on devbox/)).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Remove" })));
    expect(commands.machineRemoveService).toHaveBeenCalledWith(machine.id, api.id);
  });

  it("offers a Machine's edits disabled, with why, while they can't be made", () => {
    const older = shownMachine(
      machineView("oldbox", [shop], [], {
        state: { kind: "connected", detail: { version: "0.5.6", managesProjects: false } },
        editProblem: "Update localhost on oldbox to manage its Projects.",
      }),
    );
    const reasons = (items: ReturnType<typeof projectMenuItems>) =>
      items.flatMap((item) => (item === "separator" ? [] : [item.disabledReason ?? null]));
    expect(reasons(machineAddItems(older))).toEqual([
      "Update localhost on oldbox to manage its Projects.",
      "Update localhost on oldbox to manage its Projects.",
    ]);
    expect(new Set(reasons(projectMenuItems(shop, true, machinePlace(older))))).toEqual(
      new Set(["Update localhost on oldbox to manage its Projects."]),
    );
    expect(reasons(projectMenuItems(shop, true, machinePlace(devbox))).every((reason) => reason === null)).toBe(true);
  });
});

describe("remote paths", () => {
  it("splits a path into its folders and joins a name to it", () => {
    expect(pathCrumbs("/home/me/shop")).toEqual([
      { name: "/", path: "/" },
      { name: "home", path: "/home" },
      { name: "me", path: "/home/me" },
      { name: "shop", path: "/home/me/shop" },
    ]);
    expect(pathCrumbs("C:\\Users\\me")).toEqual([
      { name: "C:\\", path: "C:\\" },
      { name: "Users", path: "C:\\Users" },
      { name: "me", path: "C:\\Users\\me" },
    ]);
    expect(childPath("/", "srv")).toBe("/srv");
    expect(childPath("/home/me", "shop")).toBe("/home/me/shop");
    expect(childPath("C:\\Users", "me")).toBe("C:\\Users\\me");
  });
});
