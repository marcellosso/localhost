import { afterEach, describe, expect, it, vi } from "vitest";
import type { AgentStatus, Onboarding } from "./bindings";
import { viewFromSnapshot } from "./core";
import {
  checklistItems,
  isAnyAgentConnected,
  isChecklistShown,
  isWelcomeDue,
  onboardingAtLaunch,
  resetOnboarding,
  startOnboarding,
  updateOnboarding,
  useOnboarding,
} from "./onboarding";
import { project, service, status } from "./test/fixtures";

const commands = vi.hoisted(() => ({ onboarding: vi.fn(), saveOnboarding: vi.fn(async () => null) }));
vi.mock("./bindings", () => ({ commands }));

const fresh: Onboarding = { hasSeenWelcome: false, isChecklistDone: false, hasStartedService: false };

describe("onboarding at launch", () => {
  afterEach(() => {
    resetOnboarding();
    vi.clearAllMocks();
  });

  it("welcomes someone new: first launch, no Projects", async () => {
    commands.onboarding.mockResolvedValue({ onboarding: null, hasOpenedBefore: false });
    expect(await startOnboarding(0)).toBe(true);
    expect(commands.saveOnboarding).toHaveBeenCalledWith(fresh);
  });

  it("skips the welcome and the checklist for someone who opened an earlier version", async () => {
    commands.onboarding.mockResolvedValue({ onboarding: null, hasOpenedBefore: true });
    expect(await startOnboarding(0)).toBe(false);
    expect(commands.saveOnboarding).toHaveBeenCalledWith({ ...fresh, hasSeenWelcome: true, isChecklistDone: true });
  });

  it("skips both for someone who already has Projects", () => {
    expect(onboardingAtLaunch(null, false, 2)).toEqual({ ...fresh, hasSeenWelcome: true, isChecklistDone: true });
  });

  it("never welcomes again once finished or skipped", async () => {
    const seen = { ...fresh, hasSeenWelcome: true };
    commands.onboarding.mockResolvedValue({ onboarding: seen, hasOpenedBefore: false });
    expect(await startOnboarding(0)).toBe(false);
    expect(commands.saveOnboarding).not.toHaveBeenCalled();
  });

  it("keeps the checklist for someone new who added a Project before seeing the welcome", async () => {
    commands.onboarding.mockResolvedValue({ onboarding: fresh, hasOpenedBefore: false });
    expect(await startOnboarding(1)).toBe(false);
    expect(commands.saveOnboarding).toHaveBeenCalledWith({ ...fresh, hasSeenWelcome: true });
    expect(isWelcomeDue(fresh, 0)).toBe(true);
  });

  it("saves a change once, and only a change", async () => {
    commands.onboarding.mockResolvedValue({ onboarding: fresh, hasOpenedBefore: false });
    await startOnboarding(0);
    updateOnboarding({ hasSeenWelcome: true });
    updateOnboarding({ hasSeenWelcome: true });
    expect(commands.saveOnboarding).toHaveBeenCalledTimes(1);
    expect(commands.saveOnboarding).toHaveBeenCalledWith({ ...fresh, hasSeenWelcome: true });
    expect(useOnboarding).toBeDefined();
  });
});

describe("the Get started checklist", () => {
  const ticks = (items: ReturnType<typeof checklistItems>) => items.map((item) => [item.step, item.isDone]);

  it("shows once there is a Project, until it is done", () => {
    const empty = viewFromSnapshot({ projects: [], statuses: [], projectStates: [] });
    const one = viewFromSnapshot({ projects: [project("Shop", 0)], statuses: [], projectStates: [] });
    expect(isChecklistShown(fresh, empty)).toBe(false);
    expect(isChecklistShown(fresh, one)).toBe(true);
    expect(isChecklistShown({ ...fresh, isChecklistDone: true }, one)).toBe(false);
  });

  it("ticks each row from what is true", () => {
    const shop = project("Shop", 0, [service("Web")]);
    const stopped = viewFromSnapshot({ projects: [shop], statuses: [], projectStates: [] });
    expect(ticks(checklistItems(stopped, fresh, false))).toEqual([
      ["project", true],
      ["service", true],
      ["start", false],
      ["agents", false],
    ]);

    const running = viewFromSnapshot({ projects: [shop], statuses: [status(shop.services[0].id, "running")], projectStates: [] });
    expect(ticks(checklistItems(running, fresh, true))).toEqual([
      ["project", true],
      ["service", true],
      ["start", true],
      ["agents", true],
    ]);
  });

  it("keeps Start ticked once a Service has been Running, and Starting doesn't count", () => {
    const shop = project("Shop", 0, [service("Web")]);
    const starting = viewFromSnapshot({ projects: [shop], statuses: [status(shop.services[0].id, "starting")], projectStates: [] });
    const start = (onboarding: Onboarding) => checklistItems(starting, onboarding, false)[2].isDone;
    expect(start(fresh)).toBe(false);
    expect(start({ ...fresh, hasStartedService: true })).toBe(true);
  });

  it("counts an agent with localhost's server, even an older entry", () => {
    const agent = (connection: AgentStatus["connection"]) => ({ connection }) as AgentStatus;
    expect(isAnyAgentConnected([agent("notConnected"), agent("notInstalled")])).toBe(false);
    expect(isAnyAgentConnected([agent("notConnected"), agent("needsUpdate")])).toBe(true);
    expect(isAnyAgentConnected([agent("connected")])).toBe(true);
  });
});
