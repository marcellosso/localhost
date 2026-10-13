import type { Project, Service, ServiceState, ServiceStatus } from "../bindings";

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

export function status(serviceId: string, state: ServiceState): ServiceStatus {
  return { serviceId, state, run: null, awaitedDependencies: [], isRestartNeeded: false };
}
