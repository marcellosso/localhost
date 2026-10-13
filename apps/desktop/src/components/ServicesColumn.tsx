import type { CSSProperties } from "react";
import { commands, type Project, type Service } from "../bindings";
import { type CoreView, serviceState } from "../core";
import type { ServiceSelection } from "../selection";
import { StateGlyph, stateLabel } from "./StateGlyph";
import styles from "./ServicesColumn.module.css";

type Props = {
  project: Project;
  view: CoreView;
  selection: ServiceSelection;
  onSelect: (selection: ServiceSelection) => void;
};

/** The selected Project's Services, with the combined stream on top. */
export function ServicesColumn({ project, view, selection, onSelect }: Props) {
  const states = project.services.map((service) => serviceState(view, service.id));
  const anyActive = states.some((state) => state === "starting" || state === "running" || state === "stopping");
  return (
    <section className={styles.column} aria-label={`${project.name} services`}>
      <header className={styles.header}>
        <div className={styles.title}>
          <h1>{project.name}</h1>
          <span className="mono secondary">{project.path}</span>
        </div>
        {project.services.length > 0 && (
          <button
            className={anyActive ? styles.secondaryButton : styles.primaryButton}
            onClick={() => void (anyActive ? commands.stopAll(project.id) : commands.startAll(project.id))}
          >
            {anyActive ? "Stop All" : "Start All"}
          </button>
        )}
      </header>
      {project.services.length === 0 ? (
        <p className="secondary">No services.</p>
      ) : (
        <ul className={styles.list} role="listbox" aria-label="Services">
          <li
            className={styles.row}
            role="option"
            aria-selected={selection.kind === "all"}
            onClick={() => onSelect({ kind: "all" })}
          >
            <span className={styles.allGlyph} aria-hidden="true">≡</span>
            <span className={styles.name}>All services</span>
          </li>
          {project.services.map((service, index) => (
            <ServiceRow
              key={service.id}
              service={service}
              index={index}
              view={view}
              isSelected={selection.kind === "service" && selection.serviceId === service.id}
              onSelect={() => onSelect({ kind: "service", serviceId: service.id })}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function ServiceRow(props: { service: Service; index: number; view: CoreView; isSelected: boolean; onSelect: () => void }) {
  const { service, index, view, isSelected, onSelect } = props;
  const state = serviceState(view, service.id);
  const isActive = state === "starting" || state === "running" || state === "stopping";
  const port = view.statuses.get(service.id)?.run?.port ?? service.port;
  return (
    <li
      className={styles.row}
      role="option"
      aria-selected={isSelected}
      aria-label={`${service.name}, ${stateLabel(state)}`}
      onClick={onSelect}
      style={{ "--service": serviceColor(index) } as CSSProperties}
    >
      <StateGlyph state={state} />
      <span className={styles.name}>{service.name}</span>
      {port != null && <span className="mono secondary">:{port}</span>}
      <span className={`${styles.command} mono secondary`}>{service.commandLine}</span>
      <button
        className={styles.rowButton}
        disabled={state === "stopping"}
        aria-label={`${isActive ? "Stop" : "Start"} ${service.name}`}
        onClick={(event) => {
          event.stopPropagation();
          void (isActive ? commands.stopService(service.id) : commands.startService(service.id));
        }}
      >
        {isActive ? "Stop" : "Start"}
      </button>
    </li>
  );
}

/** A Service's colour, by its position in the Project. */
export function serviceColor(index: number): string {
  return `var(--service-${index % 8})`;
}
