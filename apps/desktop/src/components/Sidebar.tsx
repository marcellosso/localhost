import { useId, useState } from "react";
import type { Project } from "../bindings";
import { type CoreView, serviceState } from "../core";
import type { SidebarSelection } from "../selection";
import { importProject } from "../editors";
import { SHORTCUTS, shortcutLabel } from "../keyboard";
import { isActive, projectServicesSummary } from "../status";
import { showAlert, showSheet } from "../ui";
import { useUpdate } from "../updates";
import { ClearIcon, GearIcon, GlobeIcon, KeyboardIcon, PlusIcon, SearchIcon } from "./Icons";
import { Menu, useContextMenu } from "./Menu";
import { projectMenuItems } from "./ServicesColumn";
import { ShortcutBadge } from "./ShortcutBadge";
import { StateGlyph } from "./StateGlyph";
import { UpdateNotice } from "./UpdateNotice";
import styles from "./Sidebar.module.css";

type Props = {
  view: CoreView;
  selection: SidebarSelection | null;
  onSelect: (selection: SidebarSelection) => void;
};

/**
 * The wordmark and project search, Ports and the Projects, a newer version
 * when there is one, and a footer with Settings and how many Services run.
 */
export function Sidebar({ view, selection, onSelect }: Props) {
  const [search, setSearch] = useState("");
  const update = useUpdate();
  const query = search.trim().toLowerCase();
  const projects = query
    ? view.projects.filter((p) => p.name.toLowerCase().includes(query) || p.key.includes(query))
    : view.projects;
  const running = [...view.statuses.values()].filter((status) => isActive(status.state)).length;
  // Orca follows the selected row, as focus stays on the list.
  const listId = useId();
  const optionId = (key: string) => `${listId}-${key}`;
  const selectedKey =
    selection?.kind === "ports"
      ? "ports"
      : selection?.kind === "project" && projects.some((project) => project.id === selection.projectId)
        ? selection.projectId
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
      </ul>
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
  index: number;
  id: string;
  isSelected: boolean;
  onSelect: () => void;
};

function ProjectRow({ project, view, index, id, isSelected, onSelect }: ProjectRowProps) {
  const state = view.projectStates.get(project.id) ?? null;
  const context = useContextMenu(`${project.name} actions`, () => projectMenuItems(project));
  return (
    <li
      id={id}
      className={`${styles.row} selectable`}
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
      <ShortcutBadge kind="projects" index={index} />
      {context.menu}
    </li>
  );
}
