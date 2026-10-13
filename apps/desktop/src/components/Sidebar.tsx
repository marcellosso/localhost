import type { Project } from "../bindings";
import type { CoreView } from "../core";
import type { SidebarSelection } from "../selection";
import { isActive } from "../status";
import { showAlert } from "../ui";
import { StateGlyph, stateLabel } from "./StateGlyph";
import styles from "./Sidebar.module.css";

type Props = {
  view: CoreView;
  selection: SidebarSelection | null;
  onSelect: (selection: SidebarSelection) => void;
};

/** Ports, the Projects, and a footer with Settings and how many Services are running. */
export function Sidebar({ view, selection, onSelect }: Props) {
  const running = [...view.statuses.values()].filter((status) => isActive(status.state)).length;
  return (
    <nav className={styles.sidebar} aria-label="Sidebar">
      <ul className={styles.list} role="listbox" aria-label="Projects and Ports">
        <li
          className={styles.row}
          role="option"
          aria-selected={selection?.kind === "ports"}
          aria-label="Ports"
          onClick={() => onSelect({ kind: "ports" })}
        >
          <span className={styles.icon} aria-hidden="true">
            ⇄
          </span>
          <span className={styles.name}>Ports</span>
        </li>
        <li className={styles.heading} role="presentation">
          Projects
        </li>
        {view.projects.map((project) => (
          <ProjectRow
            key={project.id}
            project={project}
            view={view}
            isSelected={selection?.kind === "project" && selection.projectId === project.id}
            onSelect={() => onSelect({ kind: "project", projectId: project.id })}
          />
        ))}
      </ul>
      {view.projects.length === 0 && <p className={`${styles.empty} secondary`}>No projects yet.</p>}
      <footer className={styles.footer}>
        <span className="secondary">{running === 0 ? "Nothing running" : `${running} running`}</span>
        <button className="button compact" onClick={() => showAlert({ kind: "settings" })}>
          Settings…
        </button>
      </footer>
    </nav>
  );
}

function ProjectRow(props: { project: Project; view: CoreView; isSelected: boolean; onSelect: () => void }) {
  const { project, view, isSelected, onSelect } = props;
  const state = view.projectStates.get(project.id) ?? null;
  return (
    <li
      className={styles.row}
      role="option"
      aria-selected={isSelected}
      aria-label={`${project.name}, ${stateLabel(state)}`}
      onClick={onSelect}
    >
      <StateGlyph state={state} />
      <span className={styles.name}>{project.name}</span>
      <span className={`${styles.key} mono secondary`}>{project.key}</span>
    </li>
  );
}
