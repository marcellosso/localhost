import type { CSSProperties } from "react";
import { openFolder, openUrl, restart, start, startAll, stop, stopAll } from "../actions";
import type { Folder, Project, Service, StatusView } from "../bindings";
import { type CoreView, serviceStatus } from "../core";
import type { ServiceSelection } from "../selection";
import {
  hasUrlToOpen,
  isActive,
  isManual,
  isWarning,
  rowSummary,
  rowSummaryHelp,
} from "../status";
import { Menu, type MenuItem } from "./Menu";
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
  const statuses = project.services.map((service) => serviceStatus(view, service.id));
  const activeCount = statuses.filter((status) => isActive(status.state)).length;
  const failedCount = statuses.filter((status) => status.state === "failed").length;
  return (
    <section className={styles.column} aria-label={`${project.name} services`}>
      <header className={styles.header}>
        <div className={styles.title}>
          <h1>{project.name}</h1>
          <span className="mono secondary">{project.path}</span>
        </div>
        <div className={styles.toolbar}>
          <button
            className="button primary"
            disabled={project.services.length === 0}
            title="Start every service except Manual ones"
            onClick={() => void startAll(project.id)}
          >
            Start All
          </button>
          <button className="button" disabled={activeCount === 0} onClick={() => void stopAll(project.id)}>
            Stop All
          </button>
          <Menu
            label={`More actions for ${project.name}`}
            items={folderMenuItems({ kind: "project", projectId: project.id })}
          />
        </div>
      </header>
      {project.services.length === 0 ? (
        <p className="secondary">No services.</p>
      ) : (
        <ul className={styles.list} role="listbox" aria-label="Services">
          <li
            className={styles.row}
            role="option"
            aria-selected={selection.kind === "all"}
            aria-label={`All services, ${activeCount} of ${statuses.length} running${failedCount > 0 ? `, ${failedCount} failed` : ""}`}
            onClick={() => onSelect({ kind: "all" })}
          >
            <div className={styles.line}>
              <span className={styles.allGlyph} aria-hidden="true">
                ≡
              </span>
              <span className={styles.name}>All services</span>
              <span className={`${styles.summary} secondary`}>
                {activeCount} of {statuses.length}
              </span>
              {failedCount > 0 && <span className={styles.failed}>{failedCount} failed</span>}
            </div>
          </li>
          {project.services.map((service, index) => (
            <ServiceRow
              key={service.id}
              project={project}
              service={service}
              status={statuses[index]}
              index={index}
              isSelected={selection.kind === "service" && selection.serviceId === service.id}
              onSelect={() => onSelect({ kind: "service", serviceId: service.id })}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

type RowProps = {
  project: Project;
  service: Service;
  status: StatusView;
  index: number;
  isSelected: boolean;
  onSelect: () => void;
};

/** A Service: its colour, state, name and port, summary and command, and, while selected, its actions. */
function ServiceRow({ project, service, status, index, isSelected, onSelect }: RowProps) {
  const state = status.state;
  // The active Run's port, which may have moved, else the Service's Port.
  const port = isActive(state) ? (status.run?.port ?? service.port) : service.port;
  const summary = rowSummary(project, service, status);
  const summaryClass = state === "failed" ? styles.failed : isWarning(status) ? styles.warning : "secondary";
  return (
    <li
      className={`${styles.row} ${isManual(service, status) ? styles.manual : ""}`}
      role="option"
      aria-selected={isSelected}
      aria-label={`${service.name}, ${summary.startsWith(stateLabel(state)) ? summary : `${stateLabel(state)}, ${summary}`}`}
      onClick={onSelect}
      style={{ "--service": serviceColor(index) } as CSSProperties}
    >
      <div className={styles.line}>
        <StateGlyph state={state} />
        <span className={styles.name}>{service.name}</span>
        {port != null && <span className="portPill">{port}</span>}
        <span className={`${styles.summary} ${summaryClass}`} title={rowSummaryHelp(service, status)}>
          {summary}
        </span>
      </div>
      <div className={`${styles.command} mono secondary`}>{service.commandLine}</div>
      {isSelected && <ServiceActions service={service} status={status} />}
    </li>
  );
}

/** Open (or Restart) and Stop while live, Start otherwise; folder actions under `⋯`. */
function ServiceActions({ service, status }: { service: Service; status: StatusView }) {
  const isLive = status.state === "starting" || status.state === "running";
  const canOpen = hasUrlToOpen(service, status);
  const act = (action: () => Promise<void>) => (event: React.MouseEvent) => {
    event.stopPropagation();
    void action();
  };
  return (
    <div className={styles.actions}>
      {isLive && (
        <>
          {canOpen && (
            <button className="button compact primary" onClick={act(() => openUrl(service.id))}>
              Open ↗
            </button>
          )}
          <button className={`button compact ${canOpen ? "" : "primary"}`} onClick={act(() => restart(service.id))}>
            Restart
          </button>
          <button className="button compact" onClick={act(() => stop(service.id))}>
            Stop
          </button>
        </>
      )}
      {status.state === "stopping" && <span className="secondary">Stopping…</span>}
      {!isActive(status.state) && (
        <button className="button compact primary" onClick={act(() => start(service.id))}>
          Start
        </button>
      )}
      <Menu
        label={`More actions for ${service.name}`}
        items={folderMenuItems({ kind: "service", serviceId: service.id })}
      />
    </div>
  );
}

/** Open the folder in the file manager, a terminal or the editor. */
export function folderMenuItems(folder: Folder): MenuItem[] {
  return [
    { label: "Open in File Manager", onSelect: () => void openFolder(folder, "fileManager") },
    { label: "Open in Terminal", onSelect: () => void openFolder(folder, "terminal") },
    { label: "Open in Editor", onSelect: () => void openFolder(folder, "editor") },
  ];
}

/** A Service's colour, by its position in the Project. */
export function serviceColor(index: number): string {
  return `var(--service-${index % 8})`;
}
