import { commands } from "../bindings";
import { acknowledgeTelemetryNotice, PRIVACY_URL, TELEMETRY_SUMMARY, useTelemetryNoticePending } from "../telemetry";
import { showAlert } from "../ui";
import styles from "./UpdateNotice.module.css";

/**
 * Said once, at the foot of the sidebar, the first time a build that sends
 * anonymous usage data opens: what it sends and how to turn it off. Either
 * button dismisses it for good.
 */
export function TelemetryNotice() {
  if (!useTelemetryNoticePending()) return null;
  return (
    <section className={styles.notice} aria-label="Anonymous usage data">
      <p className={styles.title}>localhost sends anonymous usage data</p>
      <p className={`${styles.detail} secondary`}>
        {TELEMETRY_SUMMARY} Turn it off in Settings › General, or set <span className="mono">DO_NOT_TRACK=1</span>.
      </p>
      <div className={styles.actions}>
        <button className={styles.link} onClick={() => void commands.openUrl(PRIVACY_URL)}>
          Privacy
        </button>
        <span className={styles.spacer} />
        <button
          className="button compact"
          onClick={() => {
            acknowledgeTelemetryNotice();
            showAlert({ kind: "settings", section: "general" });
          }}
        >
          Settings…
        </button>
        <button className="button compact accent" onClick={acknowledgeTelemetryNotice}>
          OK
        </button>
      </div>
    </section>
  );
}
