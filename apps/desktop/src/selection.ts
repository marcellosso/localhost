import type { Id } from "./bindings";

/**
 * What the sidebar shows: the Ports view, a Project, or a Project of another
 * Machine, which is known by both ids.
 */
export type SidebarSelection =
  | { kind: "ports" }
  | { kind: "project"; projectId: Id }
  | { kind: "machineProject"; machineId: Id; projectId: Id };

/** What the log column shows: one Service, every Service of the Project, or nothing yet (as a Project opens). */
export type ServiceSelection = { kind: "service"; serviceId: Id } | { kind: "all" } | { kind: "none" };

export const allServices: ServiceSelection = { kind: "all" };
export const noService: ServiceSelection = { kind: "none" };
