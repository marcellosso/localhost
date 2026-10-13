import type { Id } from "./bindings";

/** What the log column shows: one Service, or every Service of the Project. */
export type ServiceSelection = { kind: "service"; serviceId: Id } | { kind: "all" };

export const allServices: ServiceSelection = { kind: "all" };
