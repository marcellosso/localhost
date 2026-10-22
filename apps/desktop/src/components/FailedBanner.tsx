import { start } from "../actions";
import type { Run, Service } from "../bindings";
import type { LogBuffer } from "../log/buffer";
import { failureExcerpt, failureTitle, runDuration } from "../status";
import { ClearIcon } from "./Icons";
import { StateGlyph } from "./StateGlyph";
import styles from "./FailedBanner.module.css";

type Props = {
  service: Service;
  run: Run;
  failureMessage: string | null;
  /** The Run's log, for the excerpt. */
  buffer: LogBuffer;
  /** Puts the banner away for this Run. */
  onHide: () => void;
};

/**
 * Why a Service Failed: what ended it and when, the error lines from its
 * output when the log below doesn't already show them all, and Start.
 */
export function FailedBanner({ service, run, failureMessage, buffer, onHide }: Props) {
  const excerpt = failureExcerpt(buffer.lines);
  const duration = runDuration(run);
  return (
    <div className={styles.banner} role="region" aria-label={`Why ${service.name} failed`}>
      <div className={styles.line}>
        <StateGlyph state="failed" />
        <span className={styles.title}>{failureTitle(service, run, failureMessage)}</span>
        {duration && <span className={`${styles.duration} secondary`}>{duration}</span>}
        <span className={styles.spacer} />
        <button className="button compact accent" onClick={() => void start(service.id)}>
          Start
        </button>
        <button className={styles.hide} aria-label="Hide" title="Hide" onClick={onHide}>
          <ClearIcon size={13} />
        </button>
      </div>
      {excerpt.length > 0 && (
        <pre className={styles.excerpt}>
          {excerpt.map((line, index) => (
            <div key={index}>{line}</div>
          ))}
        </pre>
      )}
    </div>
  );
}
