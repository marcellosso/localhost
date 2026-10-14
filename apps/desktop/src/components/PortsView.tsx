import { type CSSProperties, useEffect, useState } from "react";
import { requestStopListener } from "../actions";
import { commands, type ListenerDetails } from "../bindings";
import { type CoreView, findService } from "../core";
import { listenerSubtitle, portsText } from "../listeners";
import { useUi } from "../ui";
import { EmptyState } from "./EmptyState";
import { ChevronIcon, NetworkIcon } from "./Icons";
import { serviceColor } from "./ServicesColumn";
import styles from "./PortsView.module.css";

/** How often Listeners are rescanned while the Ports view is open. */
const REFRESH_INTERVAL = 2_000;

/**
 * Every Listener on this machine, rescanned every 2 seconds while visible:
 * development servers and Docker first, apps and system services folded
 * below. The selected one's process is shown in full on the right.
 */
export function PortsView({ view }: { view: CoreView }) {
  const [listeners, setListeners] = useState<ListenerDetails[] | null>(null);
  const [selectedPid, setSelectedPid] = useState<number | null>(null);
  const [showsOthers, setShowsOthers] = useState(false);
  const { stoppingListeners } = useUi();

  // Restarted when a stop begins or ends, so the list catches up at once.
  useEffect(() => {
    let isCancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const refresh = async () => {
      const result = await commands.listeners();
      if (isCancelled) return;
      if (result.status === "ok") setListeners(result.data);
      timer = setTimeout(refresh, REFRESH_INTERVAL);
    };
    void refresh();
    return () => {
      isCancelled = true;
      clearTimeout(timer);
    };
  }, [stoppingListeners]);

  const all = listeners ?? [];
  const development = all.filter(({ listener }) => listener.kind === "development" || listener.kind === "docker");
  const others = all.filter(({ listener }) => listener.kind === "app" || listener.kind === "system");
  const selected = all.find(({ listener }) => listener.pid === selectedPid);
  const row = (details: ListenerDetails) => (
    <ListenerRow
      key={details.listener.pid}
      details={details}
      view={view}
      isSelected={details.listener.pid === selectedPid}
      isStopping={stoppingListeners.has(details.listener.pid)}
      onSelect={() => setSelectedPid(details.listener.pid)}
    />
  );

  return (
    <>
      <section className={styles.column} aria-label="Ports" data-column="list">
        {listeners !== null && listeners.length === 0 ? (
          <EmptyState
            icon={<NetworkIcon size={40} />}
            title="No Listeners"
            description="Nothing of yours is listening on a TCP port."
          />
        ) : (
          <ul className={styles.list} role="listbox" aria-label="Listeners" tabIndex={0}>
            <li className={styles.section} role="presentation">
              Development
            </li>
            {development.map(row)}
            {listeners !== null && development.length === 0 && (
              <li className={`${styles.note} secondary`} role="presentation">
                No development servers are listening.
              </li>
            )}
            {others.length > 0 && (
              <li role="presentation">
                <button
                  className={styles.disclosure}
                  aria-expanded={showsOthers}
                  onClick={() => setShowsOthers(!showsOthers)}
                >
                  <ChevronIcon className={styles.chevron} size={9} />
                  Apps &amp; System ({others.length})
                </button>
              </li>
            )}
            {showsOthers && others.map(row)}
          </ul>
        )}
      </section>
      {selected ? (
        <ListenerDetail details={selected} isStopping={stoppingListeners.has(selected.listener.pid)} />
      ) : (
        <section className={styles.detail} aria-label="Listener" data-column="detail">
          <EmptyState
            icon={<NetworkIcon size={40} />}
            title="No Listener Selected"
            description="Select a listener to see its process."
          />
        </section>
      )}
    </>
  );
}

type RowProps = {
  details: ListenerDetails;
  view: CoreView;
  isSelected: boolean;
  isStopping: boolean;
  onSelect: () => void;
};

function ListenerRow({ details, view, isSelected, isStopping, onSelect }: RowProps) {
  const { listener, owner } = details;
  const service = listener.owner.kind === "run" ? findService(view, listener.owner.serviceId) : null;
  const color = service ? serviceColor(service.index) : undefined;
  // A Service's Listener is named by its Service; others by their process.
  const showsOwner = listener.owner.kind === "project" || listener.kind === "docker";
  return (
    <li
      className={`${styles.row} selectable`}
      role="option"
      aria-selected={isSelected}
      aria-label={[service ? owner : listener.name, portsText(listener), listenerSubtitle(listener)].join(", ")}
      onClick={onSelect}
      style={color ? ({ "--service": color } as CSSProperties) : undefined}
    >
      <span className={styles.ports}>
        {listener.ports.map((port) => (
          <span key={port} className={service ? "portPill" : "mono secondary"}>
            :{port}
          </span>
        ))}
      </span>
      <span className={styles.what}>
        <span className={styles.title}>
          {service ? (
            <span className={styles.serviceLabel}>{owner}</span>
          ) : (
            <>
              <span className={styles.name}>{listener.name}</span>
              {showsOwner && <span className="secondary">{owner}</span>}
            </>
          )}
        </span>
        <span className={`${styles.subtitle} mono secondary`}>{listenerSubtitle(listener)}</span>
      </span>
      <StopButton details={details} isStopping={isStopping} />
    </li>
  );
}

/** Stop for a Listener: its Service for a Run's, otherwise after confirming. Disabled, with why, when refused. */
function StopButton({ details, isStopping }: { details: ListenerDetails; isStopping: boolean }) {
  if (isStopping) return <span className="secondary">Stopping…</span>;
  const isRun = details.listener.owner.kind === "run";
  const reason = details.reasonNotStoppable;
  return (
    <button
      className="button compact"
      disabled={reason !== null}
      title={reason ?? (isRun ? "Stop the service" : "Stop this process")}
      aria-label={`Stop ${details.listener.name}`}
      onClick={(event) => {
        event.stopPropagation();
        requestStopListener(details);
      }}
    >
      {isRun ? "Stop" : "Stop…"}
    </button>
  );
}

function ListenerDetail({ details, isStopping }: { details: ListenerDetails; isStopping: boolean }) {
  const { listener, owner } = details;
  const fields: [string, string | null, boolean][] = [
    ["Ports", listener.ports.join(", "), true],
    ["Command", listener.command, true],
    ["Executable", listener.executablePath, true],
    ["PID", String(listener.pid), true],
    ["Process Group", String(listener.processGroupId), true],
    ["Working Directory", listener.workingDirectory ?? "Unknown", true],
    ["Belongs To", owner, false],
    ["launchd Job", listener.launchdLabel, true],
  ];
  return (
    <section className={styles.detail} aria-label={`${listener.name} process`} data-column="detail">
      <dl className={styles.form}>
        {fields.map(([label, value, isMono]) =>
          value === null ? null : (
            <div key={label} className={styles.field}>
              <dt>{label}</dt>
              <dd className={isMono ? "mono" : undefined}>{value}</dd>
            </div>
          ),
        )}
      </dl>
      {listener.owner.kind === "unmanaged" && listener.kind === "development" && (
        <p className={styles.footnote}>localhost didn’t start this process and it isn’t inside a project folder.</p>
      )}
      <div className={`${styles.form} ${styles.detailActions}`}>
        <StopButton details={details} isStopping={isStopping} />
      </div>
    </section>
  );
}
