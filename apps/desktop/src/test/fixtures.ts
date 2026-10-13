import type { Project, Run, Service, ServiceState, StatusView } from "../bindings";

let next = 0;
const id = () => `00000000-0000-4000-8000-${String(++next).padStart(12, "0")}`;

export function service(name: string, port: number | null = null): Service {
  return {
    id: id(),
    name,
    key: name.toLowerCase(),
    workingDirectory: ".",
    commandLine: `run ${name}`,
    environment: {},
    port,
  };
}

export function project(name: string, sortOrder: number, services: Service[] = []): Project {
  return { id: id(), name, key: name.toLowerCase(), path: `/src/${name}`, sortOrder, isFavorite: false, services };
}

export function status(serviceId: string, state: ServiceState, changes: Partial<StatusView> = {}): StatusView {
  return { serviceId, state, run: null, awaitedDependencies: [], isRestartNeeded: false, failureMessage: null, ...changes };
}

export function run(serviceId: string, changes: Partial<Run> = {}): Run {
  return {
    id: id(),
    serviceId,
    origin: "mainWindow",
    startedAt: { secs_since_epoch: 1_000, nanos_since_epoch: 0 },
    endedAt: null,
    termination: null,
    failure: null,
    stopOrigin: null,
    wasForceKilled: false,
    port: null,
    portChange: null,
    isPortUnresponsive: false,
    detectedUrl: null,
    ...changes,
  };
}
