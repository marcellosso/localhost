import type { ServiceState } from "../bindings";
import { stateLabel } from "../status";
import styles from "./StateGlyph.module.css";

/**
 * The state glyph. Its shape carries the state without colour: a dot with a
 * halo for Running, a ring for Starting/Stopping (pulsing, unless motion is
 * reduced), a diamond for Failed, a hollow ring for Stopped/Exited.
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
