import { type CSSProperties, useState } from "react";
import { openUrl } from "../actions";
import type { Id, LogSource, Project } from "../bindings";
import { type CoreView, serviceStatus } from "../core";
import { useLog } from "../log/useLog";
import type { ServiceSelection } from "../selection";
import { hasPortWarning, hasUrlToOpen, portChangeExplanation, statusSummary } from "../status";
import { FailedBanner } from "./FailedBanner";
import { type LogPrefix, LogView } from "./LogView";
import { serviceColor } from "./ServicesColumn";
import styles from "./LogPane.module.css";

/** The selected Service's latest Run, or every Service of the Project in one stream. */
export function LogPane({ project, view, selection }: { project: Project; view: CoreView; selection: ServiceSelection }) {
  const index = selection.kind === "service" ? project.services.findIndex((s) => s.id === selection.serviceId) : -1;
  const service = index >= 0 ? project.services[index] : undefined;
  const status = service ? serviceStatus(view, service.id) : undefined;
  const runId = status?.run?.id;
  // The Failed Run whose banner was put away.
  const [hiddenBannerRunId, setHiddenBannerRunId] = useState<Id | null>(null);

  let key: string;
  let source: LogSource | null;
  if (service) {
    key = `run:${runId ?? `none:${service.id}`}`;
    source = runId ? { kind: "run", runId } : null;
  } else {
    const serviceIds = project.services.map((s) => s.id);
    key = `services:${serviceIds.join(",")}`;
    source = { kind: "services", serviceIds };
  }
  const buffer = useLog(key, source);
  const title = service?.name ?? "All services";
  const summaryClass =
    status?.state === "failed" ? styles.failed : status && hasPortWarning(status) ? styles.warning : "secondary";

  return (
    <section className={styles.pane} aria-label={`${title} log`}>
      <header className={styles.header} style={{ "--service": serviceColor(index) } as CSSProperties}>
        {service && <span className={styles.swatch} aria-hidden="true" />}
        <span className={styles.title}>{title}</span>
        {status && (
          <span className={`${styles.summary} ${summaryClass}`} title={portChangeExplanation(status) ?? undefined}>
            {statusSummary(status)}
          </span>
        )}
        {service && status && hasUrlToOpen(service, status) && (
          <button className="button compact" onClick={() => void openUrl(service.id)}>
            Open in Browser ↗
          </button>
        )}
      </header>
      <div className={styles.body}>
        {service && status?.state === "failed" && status.run && status.run.id !== hiddenBannerRunId && (
          <FailedBanner
            service={service}
            run={status.run}
            failureMessage={status.failureMessage}
            buffer={buffer}
            onHide={() => setHiddenBannerRunId(status.run!.id)}
          />
        )}
        <LogView
          buffer={buffer}
          prefixes={service ? undefined : prefixes(project)}
          label={`${title} output`}
          emptyText={service ? `${service.name} has no output yet.` : "No output yet."}
        />
      </div>
    </section>
  );
}

/** Each Service's Key, padded to the longest, in its colour. */
export function prefixes(project: Project): Map<Id, LogPrefix> {
  const width = Math.max(0, ...project.services.map((service) => service.key.length));
  return new Map(
    project.services.map((service, index) => [service.id, { text: service.key.padEnd(width), color: serviceColor(index) }]),
  );
}
