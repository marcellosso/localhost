import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ImportPreview, Project, ServiceSuggestion } from "../bindings";
import { viewFromSnapshot } from "../core";
import { project, service } from "../test/fixtures";
import { currentUi, resetUi, showAlert, showSheet } from "../ui";
import { Alerts } from "./Alerts";
import { Sheets } from "./Sheets";

const commands = vi.hoisted(() => ({
  chooseFolder: vi.fn(),
  chooseWorkingDirectory: vi.fn(),
  scanFolder: vi.fn(),
  suggestedProjectKey: vi.fn(),
  suggestedServiceKey: vi.fn(),
  keyFromName: vi.fn(),
  addProject: vi.fn(),
  editService: vi.fn(),
  serviceRelations: vi.fn(),
  commandLineUsesPort: vi.fn(),
  secretValue: vi.fn(),
  saveEnvironment: vi.fn(),
  importProject: vi.fn(),
  removeService: vi.fn(),
}));
vi.mock("../bindings", () => ({ commands }));

const ok = <T,>(data: T) => ({ status: "ok", data }) as const;

const api = service("API", 4000);
const worker = { ...service("Worker"), dependencies: [api.id], secretNames: ["TOKEN"], environment: { MODE: "dev" } };
const shop: Project = { ...project("Shop", 0, [api, worker]), path: "/home/me/shop" };
const view = viewFromSnapshot({ projects: [shop], statuses: [], projectStates: [] });

function renderSheets() {
  const selected = { project: vi.fn(), service: vi.fn() };
  render(<Sheets view={view} onSelectProject={selected.project} onSelectService={selected.service} />);
  return selected;
}

function field(name: string) {
  return screen.getByLabelText(name) as HTMLInputElement;
}

function suggestion(name: string, isRecommended: boolean): ServiceSuggestion {
  return {
    name,
    key: name.toLowerCase(),
    workingDirectory: ".",
    commandLine: `pnpm ${name.toLowerCase()}`,
    port: null,
    portCanChange: true,
    isRecommended,
    foundIn: "package.json",
    detail: name.toLowerCase(),
  };
}

describe("Sheets", () => {
  beforeEach(() => {
    Object.values(commands).forEach((command) => command.mockResolvedValue(ok(null)));
    commands.suggestedProjectKey.mockImplementation(async (name: string) => ok(name.toLowerCase().replace(/ /g, "-")));
    commands.serviceRelations.mockResolvedValue(ok({ cycles: [], dependents: [] }));
    commands.commandLineUsesPort.mockResolvedValue(true);
  });
  afterEach(() => act(resetUi));

  it("adds a Project from a chosen folder with the Suggestions left checked", async () => {
    commands.chooseFolder.mockResolvedValue("/home/me/my-saas");
    commands.scanFolder.mockResolvedValue(ok([suggestion("Dev", true), suggestion("Build", false)]));
    const added = { ...shop, id: "new" };
    commands.addProject.mockResolvedValue(ok(added));
    const selected = renderSheets();
    act(() => showSheet({ kind: "addProject" }));

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Folder" })));
    expect(field("Name").value).toBe("my-saas");
    await waitFor(() => expect(field("Key").value).toBe("my-saas"));
    expect(screen.getByText("1 selected · edit before adding")).toBeTruthy();
    expect(screen.getByText("↩ to add with 1 service")).toBeTruthy();

    // The Key follows the name until it is edited.
    fireEvent.change(field("Name"), { target: { value: "My SaaS" } });
    await waitFor(() => expect(field("Key").value).toBe("my-saas"));
    fireEvent.change(field("Key"), { target: { value: "saas" } });
    fireEvent.change(field("Name"), { target: { value: "My Other SaaS" } });
    await act(async () => {});
    expect(field("Key").value).toBe("saas");

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Add Project" })));
    expect(commands.addProject).toHaveBeenCalledWith("My Other SaaS", "saas", "/home/me/my-saas", [
      { name: "Dev", key: "dev", workingDirectory: ".", commandLine: "pnpm dev", port: "", portCanChange: true },
    ]);
    expect(currentUi().sheet).toBeNull();
    expect(selected.project).toHaveBeenCalledWith("new");
  });

  it("keeps the sheet open with the core's message when saving fails", async () => {
    const message = "“My App” is not a valid key. Use lowercase letters, digits and single hyphens, e.g. my-app.";
    commands.editService.mockResolvedValue({ status: "error", error: { error: { kind: "invalidKey", detail: "My App" }, message } });
    renderSheets();
    act(() => showSheet({ kind: "editService", serviceId: api.id }));

    fireEvent.change(field("Key"), { target: { value: "My App" } });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(screen.getByRole("alert").textContent).toBe(message);
    expect(currentUi().sheet).not.toBeNull();
  });

  it("edits a Service: its variables, Secrets kept by name, and Dependencies", async () => {
    renderSheets();
    act(() => showSheet({ kind: "editService", serviceId: worker.id }));
    expect(screen.getByRole("dialog", { name: "Edit Worker" })).toBeTruthy();
    expect(field("Value of TOKEN").type).toBe("password");
    expect(field("Value of TOKEN").placeholder).toBe("Saved — type to replace");

    fireEvent.click(screen.getByRole("button", { name: /^API/ }));
    fireEvent.change(field("Port"), { target: { value: "8080" } });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(commands.editService).toHaveBeenCalledWith(worker.id, {
      name: "Worker",
      key: "worker",
      workingDirectory: ".",
      commandLine: "run Worker",
      port: "8080",
      url: "",
      portCanChange: true,
      variables: [
        { name: "MODE", value: { kind: "plain", value: "dev" } },
        { name: "TOKEN", value: { kind: "storedSecret", previousName: "TOKEN" } },
      ],
      dependencies: [],
      isExcludedFromStartAll: false,
    });
  });

  it("reveals a stored Secret from its layer", async () => {
    commands.secretValue.mockResolvedValue(ok("s3cret"));
    renderSheets();
    act(() => showSheet({ kind: "editService", serviceId: worker.id }));

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Reveal the value of TOKEN" })));
    expect(commands.secretValue).toHaveBeenCalledWith("TOKEN", { kind: "service", id: worker.id });
    expect(field("Value of TOKEN").value).toBe("s3cret");
    expect(field("Value of TOKEN").type).toBe("text");
  });

  it("doesn't offer a Dependency that would be a cycle, and says why", async () => {
    commands.serviceRelations.mockResolvedValue(
      ok({ cycles: [{ serviceId: worker.id, message: "api → worker → api would be a dependency cycle." }], dependents: ["Worker"] }),
    );
    renderSheets();
    act(() => showSheet({ kind: "editService", serviceId: api.id }));

    const chip = await screen.findByRole("button", { name: "Worker" });
    await waitFor(() => expect((chip as HTMLButtonElement).disabled).toBe(true));
    expect(chip.title).toBe("api → worker → api would be a dependency cycle.");
    fireEvent.click(screen.getByRole("switch", { name: "Manual" }));
    expect(screen.getByText("Start All skips this service, but still starts it for “Worker”, which depends on it.")).toBeTruthy();
  });

  it("saves the Environment: a new Profile, made active", async () => {
    renderSheets();
    act(() => showSheet({ kind: "environment", projectId: shop.id }));

    fireEvent.click(screen.getByRole("button", { name: "Add Profile" }));
    fireEvent.click(screen.getByRole("button", { name: "Development" }));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(commands.saveEnvironment).toHaveBeenCalledWith(
      shop.id,
      [],
      [{ profileId: null, name: "Development", variables: [] }],
      0,
    );
  });

  it("imports a Project file over the Project with its Key when asked to replace it", async () => {
    const preview: ImportPreview = {
      fileName: "shop.yaml",
      name: "Shop",
      key: "shop",
      path: "/elsewhere/shop",
      folderExists: false,
      services: [{ name: "API", key: "api", workingDirectory: ".", commandLine: "run API", port: 4000, isExcludedFromStartAll: false, dependencies: [] }],
      environment: "1 project variable",
      secrets: [{ name: "TOKEN", owner: "project" }],
      clash: { projectName: "Shop", freeKey: "shop-2", removed: ["Worker (running, stopped first)"] },
    };
    commands.chooseFolder.mockResolvedValue("/home/me/shop");
    commands.importProject.mockResolvedValue(ok(shop));
    const selected = renderSheets();
    act(() => showSheet({ kind: "importProject", file: "/tmp/shop.yaml", preview }));

    const importButton = screen.getByRole("button", { name: "Import" }) as HTMLButtonElement;
    expect(importButton.disabled).toBe(true);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Folder" })));
    fireEvent.click(screen.getByLabelText(/Replace “Shop”/));
    expect(screen.getByText("Removes Worker (running, stopped first), which the file doesn’t have.")).toBeTruthy();

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Replace" })));
    expect(commands.importProject).toHaveBeenCalledWith("/tmp/shop.yaml", "/home/me/shop", "replace");
    expect(selected.project).toHaveBeenCalledWith(shop.id);
  });

  it("asks before removing a Service, naming those that depend on it", async () => {
    render(<Alerts view={view} />);
    act(() => showAlert({ kind: "removeService", serviceId: api.id, dependents: ["Worker"] }));

    const dialog = screen.getByRole("alertdialog", { name: "Remove “API”?" });
    expect(dialog.textContent).toContain("“Worker” will no longer depend on it.");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Remove" })));
    expect(commands.removeService).toHaveBeenCalledWith(api.id);
  });
});
