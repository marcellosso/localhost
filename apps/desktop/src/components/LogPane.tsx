import type { Id, LogSource, Project } from "../bindings";
import type { CoreView } from "../core";
import { useLog } from "../log/useLog";
import type { ServiceSelection } from "../selection";
import { type LogPrefix, LogView } from "./LogView";
import { serviceColor } from "./ServicesColumn";
import styles from "./LogPane.module.css";

/** The selected Service's latest Run, or every Service of the Project in one stream. */
export function LogPane({ project, view, selection }: { project: Project; view: CoreView; selection: ServiceSelection }) {
  const service = selection.kind === "service" ? project.services.find((s) => s.id === selection.serviceId) : undefined;
  const runId = service ? view.statuses.get(service.id)?.run?.id : undefined;

  let key: string;
  let source: LogSource | null;
  if (service) {
    key = `run:${runId ?? `none:${service.id}`}`;
    source = runId ? { kind: "run", runId } : null;
  } else {
    const serviceIds = project.services.map((s) => s.id);
    key = `services:${serviceIds.join(",")}`;
    source = { kind: "services", serviceIds };
  }
  const buffer = useLog(key, source);
  const title = service?.name ?? "All services";

  return (
    <section className={styles.pane} aria-label={`${title} log`}>
      <header className={styles.header}>{title}</header>
      <LogView
        buffer={buffer}
        prefixes={service ? undefined : prefixes(project)}
        label={`${title} output`}
        emptyText={service ? `${service.name} has no output yet.` : "No output yet."}
      />
    </section>
  );
}

/** Each Service's Key, padded to the longest, in its colour. */
export function prefixes(project: Project): Map<Id, LogPrefix> {
  const width = Math.max(0, ...project.services.map((service) => service.key.length));
  return new Map(
    project.services.map((service, index) => [service.id, { text: service.key.padEnd(width), color: serviceColor(index) }]),
  );
}
