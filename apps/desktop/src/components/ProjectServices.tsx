import type { CSSProperties } from "react";
import type { Project } from "../bindings";
import { type CoreView, serviceState } from "../core";
import { StateGlyph } from "./StateGlyph";
import styles from "./ProjectServices.module.css";

/** The selected Project's Services. The full column comes with the main window. */
export function ProjectServices({ project, view }: { project: Project; view: CoreView }) {
  return (
    <section className={styles.column} aria-label={`${project.name} services`}>
      <header className={styles.header}>
        <h1>{project.name}</h1>
        <span className="mono secondary">{project.path}</span>
      </header>
      {project.services.length === 0 ? (
        <p className="secondary">No services.</p>
      ) : (
        <ul className={styles.list}>
          {project.services.map((service, index) => (
            <li key={service.id} className={styles.row} style={{ "--service": `var(--service-${index % 8})` } as CSSProperties}>
              <StateGlyph state={serviceState(view, service.id)} />
              <span className={styles.name}>{service.name}</span>
              {service.port != null && <span className="mono secondary">:{service.port}</span>}
              <span className={`${styles.command} mono secondary`}>{service.commandLine}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
