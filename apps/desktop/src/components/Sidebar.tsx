import type { Id, Project } from "../bindings";
import type { CoreView } from "../core";
import { StateGlyph, stateLabel } from "./StateGlyph";
import styles from "./Sidebar.module.css";

type Props = {
  view: CoreView;
  selectedId: Id | null;
  onSelect: (id: Id) => void;
};

export function Sidebar({ view, selectedId, onSelect }: Props) {
  return (
    <nav className={styles.sidebar} aria-label="Projects">
      <h2 className={styles.heading}>Projects</h2>
      {view.projects.length === 0 ? (
        <p className={`${styles.empty} secondary`}>No projects yet.</p>
      ) : (
        <ul className={styles.list} role="listbox" aria-label="Projects">
          {view.projects.map((project) => (
            <ProjectRow
              key={project.id}
              project={project}
              view={view}
              isSelected={project.id === selectedId}
              onSelect={() => onSelect(project.id)}
            />
          ))}
        </ul>
      )}
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
