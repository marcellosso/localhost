import { type CSSProperties, Fragment, useEffect, useId, useState } from "react";
import { requestStopListener } from "../actions";
import { commands, type ExecutionTarget, type ListenerDetails } from "../bindings";
import { type CoreView, findService } from "../core";
import { listenerKey, listenerSubtitle, portsText } from "../listeners";
import { spoken } from "../status";
import { distroOf, targetId } from "../targets";
import { useUi } from "../ui";
import { EmptyState } from "./EmptyState";
import { ChevronIcon, NetworkIcon } from "./Icons";
import { serviceColor } from "./ServicesColumn";
import styles from "./PortsView.module.css";

/** How often Listeners are rescanned while the Ports view is open. */
const REFRESH_INTERVAL = 2_000;

/** The Listeners of one Execution Target: development servers and Docker, then apps and system services. */
type Group = { target: ExecutionTarget; development: ListenerDetails[]; others: ListenerDetails[] };

/**
 * This machine's Listeners, then each WSL distro's (Windows only; this
 * machine's are left out while Windows itself can't be scanned and has none).
 */
function grouped(all: ListenerDetails[]): Group[] {
  const groups = new Map<string, Group>([["thisMachine", { target: { kind: "thisMachine" }, development: [], others: [] }]]);
  for (const details of all) {
    const id = targetId(details.target);
    const group = groups.get(id) ?? { target: details.target, development: [], others: [] };
    groups.set(id, group);
    const isDevelopment = details.listener.kind === "development" || details.listener.kind === "docker";
    (isDevelopment ? group.development : group.others).push(details);
  }
  const [here, ...distros] = groups.values();
  distros.sort((a, b) => (distroOf(a.target) ?? "").localeCompare(distroOf(b.target) ?? ""));
  const isHereEmpty = here.development.length === 0 && here.others.length === 0;
  return distros.length > 0 && isHereEmpty ? distros : [here, ...distros];
}

/**
 * Every Listener on this machine and in the WSL distros in use, rescanned
 * every 2 seconds while visible: development servers and Docker first,
 * apps and system services folded below, grouped by distro on Windows.
 * The selected one's process is shown in full on the right.
 */
export function PortsView({ view }: { view: CoreView }) {
  const [listeners, setListeners] = useState<ListenerDetails[] | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  /** The groups whose apps and system services are unfolded, by target. */
  const [unfolded, setUnfolded] = useState<ReadonlySet<string>>(new Set());
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
  const groups = grouped(all);
  const isGrouped = groups.length > 1 || groups[0].target.kind === "wsl";
  const selected = all.find((details) => listenerKey(details) === selectedKey);
  const isShown = (details: ListenerDetails) =>
    unfolded.has(targetId(details.target)) || groups.some((group) => group.development.includes(details));
  // Orca follows the selected row, as focus stays on the list.
  const listId = useId();
  const optionId = (key: string) => `${listId}-${key}`;
  const toggle = (id: string) => {
    const next = new Set(unfolded);
    if (!next.delete(id)) next.add(id);
    setUnfolded(next);
  };
  const row = (details: ListenerDetails) => {
    const key = listenerKey(details);
    return (
      <ListenerRow
        key={key}
        id={optionId(key)}
        details={details}
        view={view}
        isSelected={key === selectedKey}
        isStopping={stoppingListeners.has(key)}
        onSelect={() => setSelectedKey(key)}
      />
    );
  };

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
          <ul
            className={styles.list}
            role="listbox"
            aria-label="Listeners"
            aria-activedescendant={selected && isShown(selected) ? optionId(listenerKey(selected)) : undefined}
            tabIndex={0}
          >
            {groups.map(({ target, development, others }) => {
              const id = targetId(target);
              const distro = distroOf(target);
              return (
                <Fragment key={id}>
                  <li className={styles.section} role="presentation">
                    {!isGrouped ? "Development" : distro ? `${distro} (WSL)` : "Windows"}
                  </li>
                  {distro && (
                    <li className={`${styles.note} ${styles.forwarding} secondary`} role="presentation">
                      Windows reaches these ports at localhost, through WSL’s port forwarding.
                    </li>
                  )}
                  {development.map(row)}
                  {listeners !== null && development.length === 0 && (
                    <li className={`${styles.note} secondary`} role="presentation">
                      No development servers are listening.
                    </li>
                  )}
                  {others.length > 0 && (
                    <li role="presentation">
                      <button className={styles.disclosure} aria-expanded={unfolded.has(id)} onClick={() => toggle(id)}>
                        <ChevronIcon className={styles.chevron} size={9} />
                        Apps &amp; System ({others.length})
                      </button>
                    </li>
                  )}
                  {unfolded.has(id) && others.map(row)}
                </Fragment>
              );
            })}
          </ul>
        )}
      </section>
      {selected ? (
        <ListenerDetail details={selected} isStopping={stoppingListeners.has(listenerKey(selected))} />
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
  id: string;
  details: ListenerDetails;
  view: CoreView;
  isSelected: boolean;
  isStopping: boolean;
  onSelect: () => void;
};

function ListenerRow({ id, details, view, isSelected, isStopping, onSelect }: RowProps) {
  const { listener, owner } = details;
  const service = listener.owner.kind === "run" ? findService(view, listener.owner.serviceId) : null;
  const color = service ? serviceColor(service.index) : undefined;
  // A Service's Listener is named by its Service; others by their process.
  const showsOwner = listener.owner.kind === "project" || listener.kind === "docker";
  // As one sentence: who, then ports, process, and owner and state.
  const label = [
    service ? spoken(owner) : listener.name,
    portsText(listener),
    listenerSubtitle(listener),
    ...(!service && showsOwner ? [spoken(owner)] : []),
    ...(isStopping ? ["stopping"] : []),
  ];
  return (
    <li
      id={id}
      className={`${styles.row} selectable`}
      role="option"
      aria-selected={isSelected}
      aria-label={label.join(", ")}
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
      aria-label={`Stop ${isRun ? spoken(details.owner) : details.listener.name}`}
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
    ["Distro", distroOf(details.target), false],
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
      {details.target.kind === "wsl" && (
        <p className={styles.footnote}>
          It listens in WSL. Windows reaches it at localhost:{listener.ports[0]}, through WSL’s port forwarding.
        </p>
      )}
      <div className={`${styles.form} ${styles.detailActions}`}>
        <StopButton details={details} isStopping={isStopping} />
      </div>
    </section>
  );
}
