import { useId, useState } from "react";
import { report, startAll, stopAll } from "../actions";
import { commands, type Project } from "../bindings";
import { type CoreView, serviceState } from "../core";
import type { SidebarSelection } from "../selection";
import { importProject } from "../editors";
import { SHORTCUTS, shortcutLabel } from "../keyboard";
import type { ShownMachine } from "../machines";
import { isActive, projectServicesSummary } from "../status";
import { showAlert, showSheet } from "../ui";
import { useUpdate } from "../updates";
import { ClearIcon, GearIcon, GlobeIcon, KeyboardIcon, PlusIcon, SearchIcon } from "./Icons";
import { Menu, type MenuItem, useContextMenu } from "./Menu";
import { projectMenuItems } from "./ServicesColumn";
import { ShortcutBadge } from "./ShortcutBadge";
import { StateGlyph } from "./StateGlyph";
import { TelemetryNotice } from "./TelemetryNotice";
import { UpdateNotice } from "./UpdateNotice";
import styles from "./Sidebar.module.css";

type Props = {
  view: CoreView;
  /** Other Machines, each shown below this computer's Projects. */
  machines?: ShownMachine[];
  selection: SidebarSelection | null;
  onSelect: (selection: SidebarSelection) => void;
};

/**
 * The wordmark and project search, Ports and the Projects, then each other
 * Machine's, a newer version when there is one, and a footer with Settings
 * and how many Services run.
 */
export function Sidebar({ view, machines = [], selection, onSelect }: Props) {
  const [search, setSearch] = useState("");
  const update = useUpdate();
  const query = search.trim().toLowerCase();
  const matching = (projects: Project[]) =>
    query ? projects.filter((p) => p.name.toLowerCase().includes(query) || p.key.includes(query)) : projects;
  const projects = matching(view.projects);
  const running = [...view.statuses.values()].filter((status) => isActive(status.state)).length;
  // Orca follows the selected row, as focus stays on the list.
  const listId = useId();
  const optionId = (key: string) => `${listId}-${key}`;
  const machineKey = (machineId: string, projectId: string) => `${machineId}:${projectId}`;
  const selectedKey =
    selection?.kind === "ports"
      ? "ports"
      : selection?.kind === "project" && projects.some((project) => project.id === selection.projectId)
        ? selection.projectId
        : selection?.kind === "machineProject"
          ? machineKey(selection.machineId, selection.projectId)
          : null;

  return (
    <nav className={styles.sidebar} aria-label="Sidebar" data-column="sidebar">
      <header className={styles.header}>
        <span className={styles.wordmark} aria-label="localhost">
          localhost<span className={styles.colon}>:</span>
        </span>
        <div className={styles.searchRow}>
          <label className={styles.search}>
            <SearchIcon size={12} />
            <input
              type="search"
              placeholder="Search projects"
              aria-label="Search projects"
              value={search}
              spellCheck={false}
              onChange={(event) => setSearch(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") setSearch("");
                // ↩ opens the first match.
                if (event.key === "Enter" && projects[0]) onSelect({ kind: "project", projectId: projects[0].id });
              }}
            />
            {search && (
              <button className={styles.clear} aria-label="Clear Search" onClick={() => setSearch("")}>
                <ClearIcon size={13} />
              </button>
            )}
          </label>
          <Menu
            label="Add Project"
            button={{ className: "iconButton", content: <PlusIcon size={14} /> }}
            items={[
              { label: "New Project…", onSelect: () => showSheet({ kind: "addProject" }) },
              { label: "Import Project…", onSelect: () => void importProject() },
            ]}
          />
        </div>
      </header>
      <ul
        className={styles.list}
        role="listbox"
        aria-label="Projects and Ports"
        aria-activedescendant={selectedKey ? optionId(selectedKey) : undefined}
        tabIndex={0}
      >
        <li
          id={optionId("ports")}
          className={`${styles.row} selectable`}
          role="option"
          aria-selected={selection?.kind === "ports"}
          aria-label="Ports"
          onClick={() => onSelect({ kind: "ports" })}
        >
          <GlobeIcon className={styles.portsIcon} size={14} />
          <span className={styles.name}>Ports</span>
        </li>
        <li className={styles.heading} role="presentation">
          Projects
        </li>
        {projects.map((project) => (
          <ProjectRow
            key={project.id}
            project={project}
            view={view}
            index={view.projects.indexOf(project)}
            id={optionId(project.id)}
            isSelected={selection?.kind === "project" && selection.projectId === project.id}
            onSelect={() => onSelect({ kind: "project", projectId: project.id })}
          />
        ))}
        {projects.length === 0 && query && (
          <li className={`${styles.note} secondary`} role="presentation">
            No projects match “{search.trim()}”.
          </li>
        )}
        {machines.map((shown) => {
          const machineId = shown.machine.id;
          return [
            <li key={`${machineId}:heading`} className={styles.heading} role="presentation">
              {shown.machine.name}
            </li>,
            <MachineNote key={`${machineId}:note`} shown={shown} />,
            ...matching(shown.view.projects).map((project) => (
              <ProjectRow
                key={machineKey(machineId, project.id)}
                project={project}
                view={shown.view}
                id={optionId(machineKey(machineId, project.id))}
                isSelected={selection?.kind === "machineProject" && selectedKey === machineKey(machineId, project.id)}
                isDimmed={!shown.isConnected}
                menuItems={() => machineProjectMenuItems(shown, project)}
                onSelect={() => onSelect({ kind: "machineProject", machineId, projectId: project.id })}
              />
            )),
          ];
        })}
      </ul>
      <TelemetryNotice />
      <UpdateNotice update={update} />
      <footer className={styles.footer}>
        <div className={styles.footerButtons}>
          <button
            className="iconButton"
            aria-label="Settings"
            title={`Settings (${shortcutLabel(SHORTCUTS.settings)})`}
            onClick={() => showAlert({ kind: "settings" })}
          >
            <GearIcon size={15} />
          </button>
          <button
            className="iconButton"
            aria-label="Keyboard Shortcuts"
            title={`Keyboard Shortcuts (${shortcutLabel(SHORTCUTS.keyboardShortcuts)})`}
            onClick={() => showAlert({ kind: "keyboardShortcuts" })}
          >
            <KeyboardIcon size={15} />
          </button>
        </div>
        {running > 0 && <span className={`${styles.running} mono secondary`}>{running} running</span>}
      </footer>
    </nav>
  );
}

type ProjectRowProps = {
  project: Project;
  view: CoreView;
  /** Its place among this computer's Projects, for its shortcut; none on another Machine. */
  index?: number;
  id: string;
  isSelected: boolean;
  onSelect: () => void;
  /** Its Machine isn't connected: what it shows is from before. */
  isDimmed?: boolean;
  menuItems?: () => MenuItem[];
};

function ProjectRow({ project, view, index, id, isSelected, onSelect, isDimmed = false, menuItems }: ProjectRowProps) {
  const state = view.projectStates.get(project.id) ?? null;
  const context = useContextMenu(`${project.name} actions`, menuItems ?? (() => projectMenuItems(project)));
  return (
    <li
      id={id}
      className={`${styles.row} selectable ${isDimmed ? styles.dimmed : ""}`}
      role="option"
      aria-selected={isSelected}
      aria-label={`${project.name}${project.isFavorite ? ", favourite" : ""}, ${projectServicesSummary(
        project,
        project.services.map((service) => serviceState(view, service.id)),
      )}`}
      onClick={onSelect}
      onContextMenu={context.onContextMenu}
    >
      <StateGlyph state={state} />
      <span className={styles.name}>{project.name}</span>
      {project.isFavorite && (
        <span className={styles.star} aria-hidden="true">
          ★
        </span>
      )}
      {index !== undefined && <ShortcutBadge kind="projects" index={index} />}
      {context.menu}
    </li>
  );
}

/**
 * How another Machine is, under its name, while that isn't plainly
 * connected: connecting, why it can't be reached (with Reconnect when it
 * waits for the user), or that it has no Projects.
 */
function MachineNote({ shown }: { shown: ShownMachine }) {
  const isReconnecting = shown.state.kind === "connecting" && shown.view.projects.length > 0;
  const text = shown.isConnected
    ? shown.view.projects.length === 0
      ? `No projects on ${shown.machine.name}.`
      : null
    : isReconnecting
      ? null
      : shown.summary;
  if (!text) return null;
  return (
    <li className={`${styles.note} ${styles.machineNote} secondary`} role="presentation">
      <span>{text}</span>
      {shown.needsUser && (
        <button
          className="button compact"
          aria-label={`Reconnect ${shown.machine.name}`}
          onClick={() => void reconnect(shown.machine.id)}
        >
          Reconnect
        </button>
      )}
    </li>
  );
}

async function reconnect(machineId: string) {
  report(await commands.reconnectMachine(machineId));
}

/** Another Machine's Project: Start All and Stop All; it is edited there. */
function machineProjectMenuItems(shown: ShownMachine, project: Project): MenuItem[] {
  const machineId = shown.machine.id;
  const anyActive = project.services.some((service) => isActive(serviceState(shown.view, service.id)));
  return [
    ...(shown.isConnected && project.services.length > 0
      ? [{ label: "Start All", onSelect: () => void startAll(project.id, machineId) }]
      : []),
    ...(shown.isConnected && anyActive ? [{ label: "Stop All", onSelect: () => void stopAll(project.id, machineId) }] : []),
    { label: "Reconnect", onSelect: () => void reconnect(machineId) },
  ];
}
