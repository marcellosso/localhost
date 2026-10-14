import { useState } from "react";
import type { Project } from "../bindings";
import type { CoreView } from "../core";
import type { SidebarSelection } from "../selection";
import { importProject } from "../editors";
import { isActive } from "../status";
import { showAlert, showSheet } from "../ui";
import { ClearIcon, GearIcon, GlobeIcon, PlusIcon, SearchIcon } from "./Icons";
import { Menu, useContextMenu } from "./Menu";
import { projectMenuItems } from "./ServicesColumn";
import { StateGlyph, stateLabel } from "./StateGlyph";
import styles from "./Sidebar.module.css";

type Props = {
  view: CoreView;
  selection: SidebarSelection | null;
  onSelect: (selection: SidebarSelection) => void;
};

/** The wordmark and project search, Ports and the Projects, and a footer with Settings and how many Services run. */
export function Sidebar({ view, selection, onSelect }: Props) {
  const [search, setSearch] = useState("");
  const query = search.trim().toLowerCase();
  const projects = query
    ? view.projects.filter((p) => p.name.toLowerCase().includes(query) || p.key.includes(query))
    : view.projects;
  const running = [...view.statuses.values()].filter((status) => isActive(status.state)).length;

  return (
    <nav className={styles.sidebar} aria-label="Sidebar">
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
      <ul className={styles.list} role="listbox" aria-label="Projects and Ports" tabIndex={0}>
        <li
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
      <footer className={styles.footer}>
        <button
          className="iconButton"
          aria-label="Settings"
          title="Settings"
          onClick={() => showAlert({ kind: "settings" })}
        >
          <GearIcon size={15} />
        </button>
        {running > 0 && <span className={`${styles.running} mono secondary`}>{running} running</span>}
      </footer>
    </nav>
  );
}

function ProjectRow(props: { project: Project; view: CoreView; isSelected: boolean; onSelect: () => void }) {
  const { project, view, isSelected, onSelect } = props;
  const state = view.projectStates.get(project.id) ?? null;
  const context = useContextMenu(`${project.name} actions`, () => projectMenuItems(project));
  return (
    <li
      className={`${styles.row} selectable`}
      role="option"
      aria-selected={isSelected}
      aria-label={`${project.name}${project.isFavorite ? ", favourite" : ""}, ${stateLabel(state)}`}
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
      {context.menu}
    </li>
  );
}
