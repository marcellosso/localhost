import type { Id, ServiceState } from "./bindings";
import type { CoreView } from "./core";
import { findService } from "./core";
import { isActive, spokenSummary } from "./status";

// What a screen reader is told as Services settle (AGENTS.md,
// Accessibility): Running, Exited, Failed and Stopped, not Starting or
// Stopping, so a Start All isn't a stream of speech.

export type Announcement = {
  text: string;
  /** Failed: interrupts what is being said. */
  isUrgent: boolean;
};

/**
 * What changed between `previous` and `view`, worded for a screen reader:
 * "Web, Running", or "Shop, Worker, Failed, exit 1" when the Project isn't
 * `selectedProjectId`. Records the new states in `previous`.
 */
export function settledAnnouncements(
  previous: Map<Id, ServiceState>,
  view: CoreView,
  selectedProjectId: Id | null,
): Announcement[] {
  const announcements: Announcement[] = [];
  for (const status of view.statuses.values()) {
    const last = previous.get(status.serviceId) ?? "stopped";
    previous.set(status.serviceId, status.state);
    if (status.state === last || (isActive(status.state) && status.state !== "running")) continue;
    const found = findService(view, status.serviceId);
    if (!found) continue;
    const service = found.project.services[found.index];
    const subject = found.project.id === selectedProjectId ? service.name : `${found.project.name}, ${service.name}`;
    announcements.push({ text: `${subject}, ${spokenSummary(status)}`, isUrgent: status.state === "failed" });
  }
  return announcements;
}

/** Each Service's state as of `view`, so what was already so isn't announced. */
export function currentStates(view: CoreView): Map<Id, ServiceState> {
  return new Map([...view.statuses.values()].map((status) => [status.serviceId, status.state]));
}
