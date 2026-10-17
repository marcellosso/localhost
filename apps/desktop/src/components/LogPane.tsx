import { type CSSProperties, type ReactNode, useEffect, useRef, useState } from "react";
import { openUrl, start } from "../actions";
import { commands, type Id, type LogSource, type Project, type Run } from "../bindings";
import { type CoreView, serviceStatus } from "../core";
import type { LogBuffer } from "../log/buffer";
import { shortcutLabel } from "../keyboard";
import { lineMatches } from "../log/segments";
import { useLog } from "../log/useLog";
import type { ServiceSelection } from "../selection";
import { logRowHeight, useSettings } from "../settings";
import { hasPortWarning, isActive, lineText, portChangeExplanation, statusSummary } from "../status";
import { showAlert } from "../ui";
import { FailedBanner } from "./FailedBanner";
import { ArrowUpRightIcon, ClearIcon, FilterIcon, PlayIcon } from "./Icons";
import { type LogPrefix, LogView } from "./LogView";
import { Menu } from "./Menu";
import { ServiceDots } from "./ServiceDots";
import { ServiceStatsButton } from "./Stats";
import { serviceColor } from "./ServicesColumn";
import styles from "./LogPane.module.css";

const FIND = shortcutLabel({ code: "KeyF" });

/** The selected Service's latest Run, or every Service of the Project in one stream. */
export function LogPane({ project, view, selection }: { project: Project; view: CoreView; selection: ServiceSelection }) {
  const settings = useSettings();
  const index = selection.kind === "service" ? project.services.findIndex((s) => s.id === selection.serviceId) : -1;
  const service = index >= 0 ? project.services[index] : undefined;
  const status = service ? serviceStatus(view, service.id) : undefined;
  const runId = status?.run?.id;
  const [filter, setFilter] = useState("");
  // Bumped when the buffers were cleared: the stream starts over.
  const [clearCount, setClearCount] = useState(0);
  // The Failed Run whose banner was put away.
  const [hiddenBannerRunId, setHiddenBannerRunId] = useState<Id | null>(null);
  const filterField = useRef<HTMLInputElement>(null);

  let key: string;
  let source: LogSource | null;
  const serviceIds = service ? [service.id] : project.services.map((s) => s.id);
  if (service) {
    key = `run:${runId ?? `none:${service.id}`}:${clearCount}`;
    source = runId ? { kind: "run", runId } : null;
  } else {
    key = `services:${serviceIds.join(",")}:${clearCount}`;
    source = { kind: "services", serviceIds };
  }
  const buffer = useLog(key, source);
  const linePrefixes = service ? undefined : prefixes(project);

  const clearLogs = async () => {
    const result = await commands.clearLogs(serviceIds);
    if (result.status === "error") showAlert({ kind: "error", message: result.error.message });
    setClearCount((count) => count + 1);
  };

  // Ctrl+F (⌘F) finds in the log, Ctrl+L (⌃L) clears it.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const isFind = (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f";
      const isClear = event.ctrlKey && event.key.toLowerCase() === "l";
      if (!isFind && !isClear) return;
      event.preventDefault();
      if (isFind) filterField.current?.focus();
      else void clearLogs();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  const header = service && status ? (
    <>
      <span className={styles.swatch} style={{ "--service": serviceColor(index) } as CSSProperties} aria-hidden="true" />
      <span className={styles.title}>{service.name}</span>
      <span
        className={`${styles.summary} ${status.state === "failed" ? styles.failed : hasPortWarning(status) ? styles.warning : "secondary"}`}
        title={portChangeExplanation(status) ?? undefined}
      >
        {statusSummary(status)}
      </span>
      {status.run && <RunTime run={status.run} />}
      {isActive(status.state) && status.urlToOpen && (
        <button
          className={styles.url}
          title={`Open ${status.urlToOpen}`}
          aria-label={`Open ${status.urlToOpen} in Browser`}
          onClick={() => void openUrl(service.id)}
        >
          <span>{status.urlToOpen.replace(/^http:\/\//, "")}</span>
          <ArrowUpRightIcon size={9} />
        </button>
      )}
      {(status.state === "starting" || status.state === "running") && status.run && !status.run.endedAt && (
        <ServiceStatsButton service={service} index={index} />
      )}
    </>
  ) : (
    <>
      <ServiceDots active={project.services.map((s) => isActive(serviceStatus(view, s.id).state))} />
      <span className={styles.title}>All services</span>
      <span className={`${styles.summary} secondary`}>
        {project.services.filter((s) => isActive(serviceStatus(view, s.id).state)).length} of {project.services.length}{" "}
        running
      </span>
    </>
  );

  return (
    <section className={styles.pane} aria-label={`${service?.name ?? "All services"} log`} data-column="detail">
      <LogHeader
        filter={filter}
        onFilter={setFilter}
        filterField={filterField}
        onCopy={() => void copyLog(buffer, filter, linePrefixes)}
        onClear={() => void clearLogs()}
      >
        {header}
      </LogHeader>
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
        <div className={styles.log}>
          <LogView
            buffer={buffer}
            prefixes={linePrefixes}
            label={`${service?.name ?? "All services"} output`}
            filter={filter}
            rowHeight={logRowHeight(settings)}
          />
          {(service ? !status?.run : buffer.lines.length === 0) && (
            <div className={styles.empty}>
              <p className="secondary">No output yet.</p>
              {service && status?.state === "stopped" && (
                <button className="button accent" title="Start" onClick={() => void start(service.id)}>
                  <PlayIcon size={11} />
                  Start {service.name}
                </button>
              )}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

type HeaderProps = {
  children: ReactNode;
  filter: string;
  onFilter: (filter: string) => void;
  filterField: React.RefObject<HTMLInputElement | null>;
  onCopy: () => void;
  onClear: () => void;
};

/** The bar above a log: what it shows, then the filter and a `⋯` menu. */
function LogHeader({ children, filter, onFilter, filterField, onCopy, onClear }: HeaderProps) {
  const [isFocused, setIsFocused] = useState(false);
  return (
    <header className={styles.header}>
      {children}
      <span className={styles.spacer} />
      <label className={styles.filter} title={`Show only lines containing this text (${FIND})`}>
        <FilterIcon size={11} />
        <input
          ref={filterField}
          type="search"
          placeholder="Filter"
          aria-label="Filter log"
          value={filter}
          spellCheck={false}
          onFocus={() => setIsFocused(true)}
          onBlur={() => setIsFocused(false)}
          onChange={(event) => onFilter(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") onFilter("");
          }}
        />
        {filter ? (
          <button className={styles.clear} aria-label="Clear Filter" onClick={() => onFilter("")}>
            <ClearIcon size={12} />
          </button>
        ) : (
          !isFocused && <span className="keyHint">{FIND}</span>
        )}
      </label>
      <Menu
        label="More log actions"
        items={[
          { label: "Copy Log", onSelect: onCopy },
          { label: "Clear Log", onSelect: onClear },
        ]}
      />
    </header>
  );
}

/** How long the active Run has been up, or when the last one ended; kept current. */
function RunTime({ run }: { run: Run }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);
  const startedAt = run.startedAt.secs_since_epoch * 1000;
  const ended = run.endedAt ? new Date(run.endedAt.secs_since_epoch * 1000) : null;
  const help = `Started ${new Date(startedAt).toLocaleString()}${ended ? `, ended ${ended.toLocaleTimeString()}` : ""}`;
  return (
    <span className={`${styles.runTime} secondary`} title={help}>
      · {ended ? ended.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }) : upFor(now - startedAt)}
    </span>
  );
}

/** "just now", "5m", "2h 3m", "1d 4h". */
export function upFor(milliseconds: number): string {
  const minutes = Math.floor(Math.max(0, milliseconds) / 60_000);
  if (minutes < 1) return "just now";
  const [days, hours] = [Math.floor(minutes / 1440), Math.floor((minutes % 1440) / 60)];
  if (days > 0) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (hours > 0) return minutes % 60 > 0 ? `${hours}h ${minutes % 60}m` : `${hours}h`;
  return `${minutes}m`;
}

/** Copies the lines shown (the filter's matches), with their Service's Key but not the stream gutter. */
async function copyLog(buffer: LogBuffer, filter: string, linePrefixes?: ReadonlyMap<Id, LogPrefix>) {
  const text = buffer.lines
    .map((line) => ({ line, prefix: line.serviceId ? linePrefixes?.get(line.serviceId)?.text : undefined }))
    .filter(({ line, prefix }) => lineMatches(line, filter, prefix))
    .map(({ line, prefix }) => (prefix ? `${prefix} ${lineText(line)}` : lineText(line)))
    .join("\n");
  await navigator.clipboard.writeText(text);
}

/** Each Service's Key, padded to the longest, in its colour. */
export function prefixes(project: Project): Map<Id, LogPrefix> {
  const width = Math.max(0, ...project.services.map((service) => service.key.length));
  return new Map(
    project.services.map((service, index) => [service.id, { text: service.key.padEnd(width), color: serviceColor(index) }]),
  );
}
