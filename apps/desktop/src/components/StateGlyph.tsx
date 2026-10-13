import type { ServiceState } from "../bindings";
import styles from "./StateGlyph.module.css";

const labels: Record<ServiceState, string> = {
  stopped: "Stopped",
  starting: "Starting",
  running: "Running",
  stopping: "Stopping",
  exited: "Exited",
  failed: "Failed",
};

export function stateLabel(state: ServiceState | null): string {
  return state ? labels[state] : "No services";
}

/**
 * The state glyph. Its shape carries the state without colour: a dot with a
 * halo for Running, a pulsing ring for Starting/Stopping (still under reduced
 * motion), a diamond for Failed, a hollow ring for Stopped/Exited.
 * `null` is a Project without Services.
 */
export function StateGlyph({ state }: { state: ServiceState | null }) {
  const shape =
    state === "running"
      ? styles.running
      : state === "starting" || state === "stopping"
        ? styles.transitioning
        : state === "failed"
          ? styles.failed
          : state === null
            ? styles.none
            : styles.stopped;
  const label = stateLabel(state);
  return <span className={`${styles.glyph} ${shape}`} role="img" aria-label={label} title={label} />;
}
