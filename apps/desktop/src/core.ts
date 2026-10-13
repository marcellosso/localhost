import type { CoreEvent, Id, Project, ServiceState, Snapshot, StatusView } from "./bindings";

/** What the UI shows: the core's state as of the last event. */
export type CoreView = {
  /** In sidebar order. */
  projects: Project[];
  /** Services that never ran have no status: they are Stopped. */
  statuses: ReadonlyMap<Id, StatusView>;
  /** Each Project's most significant Service state; `null` without Services. */
  projectStates: ReadonlyMap<Id, ServiceState | null>;
};

export function viewFromSnapshot(snapshot: Snapshot): CoreView {
  return {
    projects: sortProjects(snapshot.projects),
    statuses: new Map(snapshot.statuses.map((status) => [status.serviceId, status])),
    projectStates: new Map(snapshot.projectStates.map(({ projectId, state }) => [projectId, state])),
  };
}

/** The view after `event`. Unchanged parts keep their identity. */
export function applyEvent(view: CoreView, event: CoreEvent): CoreView {
  switch (event.kind) {
    case "library": {
      const change = event.event;
      switch (change.kind) {
        case "projectAdded":
          return { ...view, projects: sortProjects([...view.projects, change.project]) };
        case "projectUpdated":
          return {
            ...view,
            projects: sortProjects(view.projects.map((p) => (p.id === change.project.id ? change.project : p))),
          };
        case "projectRemoved": {
          const projectStates = new Map(view.projectStates);
          projectStates.delete(change.id);
          return { ...view, projects: view.projects.filter((p) => p.id !== change.id), projectStates };
        }
      }
      break;
    }
    case "status": {
      const status = event.event;
      return { ...view, statuses: new Map(view.statuses).set(status.serviceId, status) };
    }
    case "projectState": {
      const { projectId, state } = event.event;
      return { ...view, projectStates: new Map(view.projectStates).set(projectId, state) };
    }
  }
  return view;
}

export function serviceState(view: CoreView, serviceId: Id): ServiceState {
  return view.statuses.get(serviceId)?.state ?? "stopped";
}

/** The Service's status; Stopped with no Run if it never ran. */
export function serviceStatus(view: CoreView, serviceId: Id): StatusView {
  return (
    view.statuses.get(serviceId) ?? {
      serviceId,
      state: "stopped",
      run: null,
      awaitedDependencies: [],
      isRestartNeeded: false,
      failureMessage: null,
    }
  );
}

/** The Project holding the Service, and the Service's position in it (its colour). */
export function findService(view: CoreView, serviceId: Id): { project: Project; index: number } | null {
  for (const project of view.projects) {
    const index = project.services.findIndex((service) => service.id === serviceId);
    if (index >= 0) return { project, index };
  }
  return null;
}

/** The core's order: by sort order, then name. */
function sortProjects(projects: Project[]): Project[] {
  return projects.toSorted((a, b) => a.sortOrder - b.sortOrder || compare(a.name, b.name));
}

function compare(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}
