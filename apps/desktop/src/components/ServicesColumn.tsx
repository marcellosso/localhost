import type { CSSProperties } from "react";
import { openFolder, openUrl, restart, start, startAll, stop, stopAll } from "../actions";
import type { Folder, Project, Service, StatusView } from "../bindings";
import { exportProject, requestRemoveService, setActiveProfile, setFavorite } from "../editors";
import { type CoreView, serviceStatus } from "../core";
import type { ServiceSelection } from "../selection";
import { isActive, isManual, isWarning, rowSummary, rowSummaryHelp } from "../status";
import { SHORTCUTS, shortcutLabel } from "../keyboard";
import { showAlert, showSheet } from "../ui";
import { EmptyState } from "./EmptyState";
import { ArrowUpRightIcon, LayersIcon, PlayIcon, PlusIcon, StopIcon, TerminalIcon } from "./Icons";
import { Menu, type MenuItem, useContextMenu } from "./Menu";
import { ServiceDots } from "./ServiceDots";
import { ShortcutBadge } from "./ShortcutBadge";
import { StateGlyph, stateLabel } from "./StateGlyph";
import styles from "./ServicesColumn.module.css";

type Props = {
  project: Project;
  view: CoreView;
  selection: ServiceSelection;
  onSelect: (selection: ServiceSelection) => void;
};

/** The selected Project's Services with their state and actions, the combined stream on top. */
export function ServicesColumn({ project, view, selection, onSelect }: Props) {
  if (project.services.length === 0) {
    return (
      <section className={styles.column} aria-label={`${project.name} services`} data-column="list">
        <EmptyState
          icon={<TerminalIcon size={40} />}
          title="No Services"
          description="A service is a command that runs in this project, like “pnpm dev”."
        >
          <button className="button accent" onClick={() => showSheet({ kind: "detectServices", projectId: project.id })}>
            Detect Services…
          </button>
          <button className="button" onClick={() => showSheet({ kind: "addService", projectId: project.id })}>
            Add Service…
          </button>
        </EmptyState>
      </section>
    );
  }
  const statuses = project.services.map((service) => serviceStatus(view, service.id));
  const active = statuses.map((status) => isActive(status.state));
  const activeCount = active.filter(Boolean).length;
  const failedCount = statuses.filter((status) => status.state === "failed").length;
  return (
    <section className={styles.column} aria-label={`${project.name} services`} data-column="list">
      <ul className={styles.list} role="listbox" aria-label="Services" tabIndex={0}>
        <li
          className={`${styles.row} selectable`}
          role="option"
          aria-selected={selection.kind === "all"}
          aria-label={`All services, ${activeCount} of ${statuses.length} running${failedCount > 0 ? `, ${failedCount} failed` : ""}`}
          onClick={() => onSelect({ kind: "all" })}
        >
          <div className={styles.line}>
            <ServiceDots active={active} />
            <span className={styles.name}>All services</span>
            <span className={`${styles.summary} secondary`}>
              {activeCount} of {statuses.length}
            </span>
            {failedCount > 0 && <span className={`${styles.summaryEnd} ${styles.failed}`}>{failedCount} failed</span>}
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
      <button
        className={`${styles.addService} secondary`}
        onClick={() => showSheet({ kind: "addService", projectId: project.id })}
      >
        <PlusIcon size={11} />
        <span className={styles.addServiceTitle}>Add Service</span>
        <span className="keyHint">{shortcutLabel(SHORTCUTS.newService)}</span>
      </button>
    </section>
  );
}

/** The selected Project's toolbar actions: its Environment Profile once it has one, Start All, Stop All, and the rest in `⋯`. */
export function ProjectActions({ project, view }: { project: Project; view: CoreView }) {
  const anyActive = project.services.some((service) => isActive(serviceStatus(view, service.id).state));
  return (
    <>
      {(project.profiles ?? []).length > 0 && <ProfileMenu project={project} />}
      <button
        className="button accent"
        disabled={project.services.length === 0}
        title={`Start every service except Manual ones (${shortcutLabel(SHORTCUTS.startAll)})`}
        onClick={() => void startAll(project.id)}
      >
        <PlayIcon size={11} />
        Start All
      </button>
      <button className="button" disabled={!anyActive} title={`Stop All (${shortcutLabel(SHORTCUTS.stopAll)})`} onClick={() => void stopAll(project.id)}>
        <StopIcon size={11} />
        Stop All
      </button>
      <Menu label="More actions for this project" items={projectMenuItems(project, true)} />
    </>
  );
}

/** Switches the active Environment Profile; opens the Environment editor. */
function ProfileMenu({ project }: { project: Project }) {
  const profiles = project.profiles ?? [];
  const active = profiles.find((profile) => profile.id === project.activeProfileID);
  return (
    <Menu
      label="Environment profile"
      button={{
        className: "button",
        content: (
          <>
            <LayersIcon size={12} />
            {active?.name ?? "No Profile"}
          </>
        ),
      }}
      items={[
        { label: "No Profile", isChecked: !active, onSelect: () => void setActiveProfile(project.id, null) },
        ...profiles.map((profile) => ({
          label: profile.name,
          isChecked: profile.id === active?.id,
          onSelect: () => void setActiveProfile(project.id, profile.id),
        })),
        "separator",
        { label: "Edit Environment…", onSelect: () => showSheet({ kind: "environment", projectId: project.id }) },
      ]}
    />
  );
}

/** A Project's actions, in the toolbar's `⋯` (with Detect Services…) and the sidebar's context menu. */
export function projectMenuItems(project: Project, withDetect = false): MenuItem[] {
  const projectId = project.id;
  return [
    { label: "Edit Project…", onSelect: () => showSheet({ kind: "editProject", projectId }) },
    { label: "Edit Environment…", onSelect: () => showSheet({ kind: "environment", projectId }) },
    ...(withDetect ? [{ label: "Detect Services…", onSelect: () => showSheet({ kind: "detectServices", projectId }) }] : []),
    {
      label: project.isFavorite ? "Unfavorite" : "Favorite",
      onSelect: () => void setFavorite(project, !project.isFavorite),
    },
    { label: "Export Project…", onSelect: () => void exportProject(projectId) },
    "separator",
    ...folderMenuItems({ kind: "project", projectId }),
    "separator",
    { label: "Remove Project…", isDestructive: true, onSelect: () => showAlert({ kind: "removeProject", projectId }) },
  ];
}

type RowProps = {
  project: Project;
  service: Service;
  status: StatusView;
  index: number;
  isSelected: boolean;
  onSelect: () => void;
};

/** A Service: a rail in its colour, its state, name and Port, its command, and, while selected, its actions. */
function ServiceRow({ project, service, status, index, isSelected, onSelect }: RowProps) {
  const state = status.state;
  // The active Run's port, which may have moved, else the Service's Port.
  const port = isActive(state) ? (status.run?.port ?? service.port) : service.port;
  const summary = rowSummary(project, service, status);
  const summaryClass = state === "failed" ? styles.failed : isWarning(status) ? styles.warning : "secondary";
  const edit = () => showSheet({ kind: "editService", serviceId: service.id });
  const context = useContextMenu(`${service.name} actions`, () => [
    ...stateMenuItems(service, status),
    "separator",
    { label: "Edit Service…", onSelect: edit },
    "separator",
    ...folderMenuItems({ kind: "service", serviceId: service.id }),
    "separator",
    { label: "Remove Service…", isDestructive: true, onSelect: () => void requestRemoveService(service.id) },
  ]);
  return (
    <li
      className={`${styles.row} selectable ${isManual(service, status) ? styles.manual : ""}`}
      role="option"
      aria-selected={isSelected}
      aria-label={`${service.name}, ${summary.startsWith(stateLabel(state)) ? summary : `${stateLabel(state)}, ${summary}`}`}
      onClick={onSelect}
      onDoubleClick={edit}
      onContextMenu={context.onContextMenu}
      style={{ "--service": serviceColor(index) } as CSSProperties}
    >
      <div className={styles.line}>
        <StateGlyph state={state} />
        <span className={styles.name}>{service.name}</span>
        {port != null && <span className="portPill">:{port}</span>}
        <span className={`${styles.summary} ${summaryClass}`} title={rowSummaryHelp(service, status)}>
          {summary}
        </span>
      </div>
      <div className={`${styles.command} mono secondary`}>{service.commandLine}</div>
      {isSelected && <ServiceActions service={service} status={status} />}
      <ShortcutBadge kind="services" index={index} />
      {context.menu}
    </li>
  );
}

/** Open (or Restart) and Stop while live, Start otherwise; the main one tinted. */
function ServiceActions({ service, status }: { service: Service; status: StatusView }) {
  const act = (action: () => Promise<void>) => (event: React.MouseEvent) => {
    event.stopPropagation();
    void action();
  };
  const url = status.urlToOpen;
  if (status.state === "starting" || status.state === "running") {
    return (
      <div className={styles.actions}>
        {url && (
          <button
            className="button compact prominent"
            title={`Open ${url} (${shortcutLabel(SHORTCUTS.openUrl)})`}
            aria-label="Open in Browser"
            onClick={act(() => openUrl(service.id))}
          >
            Open <ArrowUpRightIcon size={9} />
          </button>
        )}
        <button
          className={`button compact ${url ? "" : "prominent"}`}
          title={`Restart (${shortcutLabel(SHORTCUTS.startOrRestart)})`}
          onClick={act(() => restart(service.id))}
        >
          Restart
        </button>
        <button className="button compact" title={`Stop (${shortcutLabel(SHORTCUTS.stop)})`} onClick={act(() => stop(service.id))}>
          Stop
        </button>
      </div>
    );
  }
  if (status.state === "stopping") {
    return (
      <div className={styles.actions}>
        <span className="secondary">Stopping…</span>
      </div>
    );
  }
  return (
    <div className={styles.actions}>
      <button
        className="button compact prominent"
        title={`Start (${shortcutLabel(SHORTCUTS.startOrRestart)})`}
        onClick={act(() => start(service.id))}
      >
        Start
      </button>
      <button
        className="button compact"
        title="Edit Service"
        onClick={act(async () => showSheet({ kind: "editService", serviceId: service.id }))}
      >
        Edit…
      </button>
    </div>
  );
}

/** What a Service's context menu offers for its state, like its buttons. */
function stateMenuItems(service: Service, status: StatusView): MenuItem[] {
  switch (status.state) {
    case "starting":
    case "running":
      return [
        ...(status.urlToOpen ? [{ label: "Open in Browser", onSelect: () => void openUrl(service.id) }] : []),
        { label: "Restart", onSelect: () => void restart(service.id) },
        { label: "Stop", onSelect: () => void stop(service.id) },
      ];
    case "stopping":
      return [];
    default:
      return [{ label: "Start", onSelect: () => void start(service.id) }];
  }
}

/** Open the folder in the file manager, the editor or a terminal. */
export function folderMenuItems(folder: Folder): MenuItem[] {
  return [
    { label: "Open in File Manager", onSelect: () => void openFolder(folder, "fileManager") },
    { label: "Open in Editor", onSelect: () => void openFolder(folder, "editor") },
    { label: "Open in Terminal", onSelect: () => void openFolder(folder, "terminal") },
  ];
}

/** A Service's colour, by its position in the Project. */
export function serviceColor(index: number): string {
  return `var(--service-${index % 8})`;
}
