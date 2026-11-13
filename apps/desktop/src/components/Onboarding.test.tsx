import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { viewFromSnapshot } from "../core";
import { resetOnboarding, showChecklistAgain, startOnboarding } from "../onboarding";
import { project, service, status } from "../test/fixtures";
import { currentUi, resetUi } from "../ui";
import { GetStarted } from "./GetStarted";
import { helpMenuItems } from "./Sidebar";
import { Welcome } from "./Welcome";

const commands = vi.hoisted(() => ({
  onboarding: vi.fn(),
  saveOnboarding: vi.fn(async () => null),
  agentStatuses: vi.fn(async () => [] as { connection: string }[]),
}));
vi.mock("../bindings", () => ({ commands }));

const fresh = { hasSeenWelcome: false, isChecklistDone: false, hasStartedService: false };

async function launch(onboarding = fresh) {
  commands.onboarding.mockResolvedValue({ onboarding, hasOpenedBefore: false });
  await act(() => startOnboarding(0));
}

afterEach(() => {
  resetOnboarding();
  resetUi();
  vi.clearAllMocks();
});

describe("Welcome", () => {
  it("walks through Project, Service and Port, and ends in Add Project…", async () => {
    await launch();
    render(<Welcome />);
    expect(screen.getByRole("heading", { name: "A project is a folder" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "A service is a command that keeps running" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "The port tells localhost it’s up" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Add Project…" }));
    expect(currentUi().sheet).toEqual({ kind: "addProject" });
    expect(commands.saveOnboarding).toHaveBeenLastCalledWith({ ...fresh, hasSeenWelcome: true });
  });

  it("is skipped for good from any step, Escape too", async () => {
    await launch();
    render(<Welcome />);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(currentUi().sheet).toBeNull();
    expect(commands.saveOnboarding).toHaveBeenLastCalledWith({ ...fresh, hasSeenWelcome: true });
  });
});

describe("GetStarted", () => {
  const shop = project("Shop", 0, [service("Web")]);
  const blog = project("Blog", 1);

  it("ticks what is done and opens what isn't", async () => {
    await launch({ ...fresh, hasSeenWelcome: true });
    const view = viewFromSnapshot({ projects: [blog], statuses: [], projectStates: [] });
    render(<GetStarted view={view} selectedProjectId={blog.id} onSelectProject={vi.fn()} />);

    expect(screen.getByText("1 of 4")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add a service" }));
    expect(currentUi().sheet).toEqual({ kind: "detectServices", projectId: blog.id });
    fireEvent.click(screen.getByRole("button", { name: "Connect your agents (optional)" }));
    expect(currentUi().sheet).toEqual({ kind: "connectAgents" });
  });

  it("remembers the first Running Service, and goes away once every row is ticked", async () => {
    await launch({ ...fresh, hasSeenWelcome: true });
    commands.agentStatuses.mockResolvedValue([{ connection: "connected" }]);
    const view = viewFromSnapshot({ projects: [shop], statuses: [status(shop.services[0].id, "running")], projectStates: [] });
    const { container } = render(<GetStarted view={view} selectedProjectId={null} onSelectProject={vi.fn()} />);

    await waitFor(() => expect(container.innerHTML).toBe(""));
    expect(commands.saveOnboarding).toHaveBeenCalledWith({ hasSeenWelcome: true, hasStartedService: true, isChecklistDone: false });
    expect(commands.saveOnboarding).toHaveBeenLastCalledWith({ hasSeenWelcome: true, hasStartedService: true, isChecklistDone: true });
  });

  it("closes for good", async () => {
    await launch({ ...fresh, hasSeenWelcome: true });
    const view = viewFromSnapshot({ projects: [shop], statuses: [], projectStates: [] });
    const { container } = render(<GetStarted view={view} selectedProjectId={null} onSelectProject={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Hide Get Started" }));
    expect(container.innerHTML).toBe("");
    expect(commands.saveOnboarding).toHaveBeenLastCalledWith({ ...fresh, hasSeenWelcome: true, isChecklistDone: true });
  });

  it("shows nothing to someone who used localhost before", async () => {
    await launch({ hasSeenWelcome: true, isChecklistDone: true, hasStartedService: false });
    const view = viewFromSnapshot({ projects: [shop], statuses: [], projectStates: [] });
    const { container } = render(<GetStarted view={view} selectedProjectId={null} onSelectProject={vi.fn()} />);
    expect(container.innerHTML).toBe("");
  });
});

describe("Help", () => {
  const shop = project("Shop", 0, [service("Web")]);
  const used = { hasSeenWelcome: true, isChecklistDone: true, hasStartedService: true };
  const help = (label: string) => {
    const item = helpMenuItems().find((item) => item !== "separator" && item.label === label);
    if (!item || item === "separator") throw new Error(`no ${label}`);
    return item;
  };

  it("shows the welcome again with Projects present, and leaves the saved state as it was", async () => {
    commands.onboarding.mockResolvedValue({ onboarding: used, hasOpenedBefore: true });
    await act(() => startOnboarding(1));
    act(() => help("Show Welcome…").onSelect());
    expect(currentUi().sheet).toEqual({ kind: "welcome" });

    render(<Welcome />);
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(currentUi().sheet).toBeNull();
    expect(commands.saveOnboarding).not.toHaveBeenCalled();
  });

  it("brings back the checklist, ticked from what is true, until closed again", async () => {
    commands.onboarding.mockResolvedValue({ onboarding: used, hasOpenedBefore: true });
    commands.agentStatuses.mockResolvedValue([{ connection: "connected" }]);
    await act(() => startOnboarding(1));
    const view = viewFromSnapshot({ projects: [shop], statuses: [status(shop.services[0].id, "running")], projectStates: [] });
    const { container } = render(<GetStarted view={view} selectedProjectId={null} onSelectProject={vi.fn()} />);
    expect(container.innerHTML).toBe("");

    act(() => help("Show Get Started Checklist").onSelect());
    expect(commands.saveOnboarding).toHaveBeenLastCalledWith({ ...used, isChecklistDone: false });
    // Every row is ticked, yet it stays: it was asked for.
    await waitFor(() => expect(screen.getByText("4 of 4")).toBeTruthy());
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    expect(screen.getByText("4 of 4")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Hide Get Started" }));
    expect(container.innerHTML).toBe("");
    expect(commands.saveOnboarding).toHaveBeenLastCalledWith(used);
  });

  it("shows a closed checklist again even before it was complete", async () => {
    commands.onboarding.mockResolvedValue({ onboarding: { ...used, hasStartedService: false }, hasOpenedBefore: true });
    await act(() => startOnboarding(1));
    const view = viewFromSnapshot({ projects: [shop], statuses: [], projectStates: [] });
    render(<GetStarted view={view} selectedProjectId={null} onSelectProject={vi.fn()} />);
    act(() => showChecklistAgain());
    expect(screen.getByText("2 of 4")).toBeTruthy();
  });
});
