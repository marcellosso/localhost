import { commands, type UpdateState } from "../bindings";
import { openReleaseNotes } from "../updates";
import styles from "./UpdateNotice.module.css";

/**
 * A newer version, at the foot of the sidebar: Update Now downloads it in
 * the background, then Restart to Update installs it. Where the package
 * manager installs it (.deb, .rpm), Download opens the release instead.
 * Nothing shows when there is no update.
 */
export function UpdateNotice({ update }: { update: UpdateState }) {
  if (update.kind === "none") return null;
  const notes = (
    <button className={styles.link} onClick={() => void openReleaseNotes(update.notesUrl)}>
      Release Notes
    </button>
  );
  return (
    <section className={styles.notice} aria-label="Update">
      {update.kind === "available" && (
        <>
          <p className={styles.title}>localhost {update.version} is available</p>
          {update.problem ? (
            <p className={styles.problem} role="alert">
              {update.problem}
            </p>
          ) : (
            !update.canInstall && <p className={`${styles.detail} secondary`}>Install it with your package manager.</p>
          )}
          <div className={styles.actions}>
            {notes}
            <span className={styles.spacer} />
            <button className="button compact" onClick={() => void commands.putOffUpdate()}>
              Later
            </button>
            {update.canInstall ? (
              <button className="button compact accent" onClick={() => void commands.downloadUpdate()}>
                {update.problem ? "Try Again" : "Update Now"}
              </button>
            ) : (
              <button className="button compact accent" onClick={() => void openReleaseNotes(update.notesUrl)}>
                Download
              </button>
            )}
          </div>
        </>
      )}
      {update.kind === "downloading" && (
        <>
          <p className={styles.title}>Downloading localhost {update.version}…</p>
          <progress
            className={styles.progress}
            aria-label={`Downloading localhost ${update.version}`}
            max={1}
            value={update.progress ?? undefined}
          />
        </>
      )}
      {update.kind === "ready" && (
        <>
          <p className={styles.title}>localhost {update.version} is ready</p>
          <p className={`${styles.detail} secondary`}>Restarting stops running services first.</p>
          <div className={styles.actions}>
            {notes}
            <span className={styles.spacer} />
            <button className="button compact accent" onClick={() => void commands.installUpdate()}>
              Restart to Update
            </button>
          </div>
        </>
      )}
    </section>
  );
}
