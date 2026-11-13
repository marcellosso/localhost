import { useSyncExternalStore } from "react";
import { type AgentStatus, commands, type Onboarding } from "./bindings";
import type { CoreView } from "./core";

// The first run: a short welcome, then a Get started checklist until the
// first Service has run. Both are for people new to localhost; someone who
// opened it before, or already has Projects, sees neither. The words are
// the macOS app's too (`Onboarding.swift`): change both together.

/** One step of the welcome. `body` marks commands with backticks. */
export type WelcomeStep = { title: string; body: string };

export const WELCOME_TITLE = "Welcome to localhost";

/** The empty states' one line about what they ask for. */
export const PROJECT_CONCEPT = "A project is a folder on this computer. Add one and localhost suggests the services that run it.";
export const SERVICE_CONCEPT = "A service is a command that keeps running in this project, like “pnpm dev” or “docker compose up”.";

export const WELCOME_STEPS: readonly WelcomeStep[] = [
  {
    title: "A project is a folder",
    body: "Add a project folder once. localhost remembers how it runs, so you start it from here instead of a row of terminal tabs.",
  },
  {
    title: "A service is a command that keeps running",
    body: "Like `pnpm dev` or `docker compose up`. When you add a folder, localhost suggests its services for you to review.",
  },
  {
    title: "The port tells localhost it’s up",
    body: "Give a service the port it listens on. Once the port answers, the service is Running, and Open in Browser goes straight to it.",
  },
];

/** The checklist's rows, in order. */
export type ChecklistStep = "project" | "service" | "start" | "agents";

export type ChecklistItem = { step: ChecklistStep; title: string; isDone: boolean; isOptional: boolean };

export const CHECKLIST_TITLE = "Get started";

/**
 * Where the first run stands at the first launch that has it: nothing to
 * show for someone who opened localhost before or already has Projects.
 * After that, what was saved.
 */
export function onboardingAtLaunch(saved: Onboarding | null, hasOpenedBefore: boolean, projectCount: number): Onboarding {
  if (saved) return saved;
  const isReturning = hasOpenedBefore || projectCount > 0;
  return { hasSeenWelcome: isReturning, isChecklistDone: isReturning, hasStartedService: false };
}

/** The welcome opens only before the first Project: one added another way makes it moot. */
export function isWelcomeDue(onboarding: Onboarding, projectCount: number): boolean {
  return !onboarding.hasSeenWelcome && projectCount === 0;
}

/** Each row, ticked from what is true now; "Start" stays ticked once a Service has been Running. */
export function checklistItems(view: CoreView, onboarding: Onboarding, hasConnectedAgent: boolean): ChecklistItem[] {
  return [
    { step: "project", title: "Add a project", isDone: view.projects.length > 0, isOptional: false },
    {
      step: "service",
      title: "Add a service",
      isDone: view.projects.some((project) => project.services.length > 0),
      isOptional: false,
    },
    {
      step: "start",
      title: "Start your first service",
      isDone: onboarding.hasStartedService || hasRunningService(view),
      isOptional: false,
    },
    { step: "agents", title: "Connect your agents", isDone: hasConnectedAgent, isOptional: true },
  ];
}

/** Shown once there is a Project, until dismissed or complete. */
export function isChecklistShown(onboarding: Onboarding, view: CoreView): boolean {
  return !onboarding.isChecklistDone && view.projects.length > 0;
}

export function hasRunningService(view: CoreView): boolean {
  return [...view.statuses.values()].some((status) => status.state === "running");
}

/** An agent counts once its configuration has localhost, even an older entry. */
export function isAnyAgentConnected(agents: readonly AgentStatus[]): boolean {
  return agents.some((agent) => agent.connection === "connected" || agent.connection === "needsUpdate");
}

// The saved state, for the window. `null` until the launch has decided,
// so nothing shows before then.

let current: Onboarding | null = null;
const listeners = new Set<() => void>();

function publish(next: Onboarding) {
  current = next;
  listeners.forEach((listener) => listener());
}

/** At launch, once the Projects are known: whether to open the welcome. */
export async function startOnboarding(projectCount: number): Promise<boolean> {
  const launch = await commands.onboarding();
  let onboarding = onboardingAtLaunch(launch.onboarding, launch.hasOpenedBefore, projectCount);
  const isDue = isWelcomeDue(onboarding, projectCount);
  if (!onboarding.hasSeenWelcome && !isDue) onboarding = { ...onboarding, hasSeenWelcome: true };
  publish(onboarding);
  if (onboarding !== launch.onboarding) void commands.saveOnboarding(onboarding);
  return isDue;
}

export function updateOnboarding(change: Partial<Onboarding>) {
  if (!current) return;
  const next = { ...current, ...change };
  if (Object.entries(change).every(([key, value]) => current?.[key as keyof Onboarding] === value)) return;
  publish(next);
  void commands.saveOnboarding(next);
}

export function useOnboarding(): Onboarding | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}

/** For tests. */
export function resetOnboarding() {
  current = null;
}
