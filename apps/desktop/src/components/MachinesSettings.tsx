import { useEffect, useId, useState } from "react";
import { commands } from "../bindings";
import { report } from "../actions";
import { type ShownMachine, useMachines } from "../machines";
import { Menu } from "./Menu";
import settings from "./SettingsDialog.module.css";
import styles from "./MachinesSettings.module.css";

export const MACHINES_TITLE = "Machines";
export const MACHINES_SUBTITLE =
  "Other computers whose projects you run from here, over SSH. Their projects, secrets and services stay on them.";

/**
 * Settings › Machines: each other computer with where `ssh` reaches it and
 * how it is, and Add Machine. Everything happens here, in the window:
 * renaming and removing ask in the row itself.
 */
export function MachinesSettings() {
  const machines = useMachines();
  const [isAdding, setIsAdding] = useState(false);
  return (
    <>
      <header className={settings.sectionHeader}>
        <h3>{MACHINES_TITLE}</h3>
        <p>{MACHINES_SUBTITLE}</p>
      </header>
      {machines.map((shown) => (
        <MachineRow key={shown.machine.id} shown={shown} />
      ))}
      {machines.length === 0 && !isAdding && (
        <p className={`${settings.row} ${styles.empty} secondary`}>
          No machines yet. Add one you can reach with ssh, like a dev box on your network or over Tailscale.
        </p>
      )}
      {isAdding ? (
        <AddMachine onDone={() => setIsAdding(false)} />
      ) : (
        <div className={styles.footer}>
          <button className="button" onClick={() => setIsAdding(true)}>
            Add Machine…
          </button>
        </div>
      )}
    </>
  );
}

/** A Machine: its name, destination and state, and Reconnect, Rename and Remove. */
function MachineRow({ shown }: { shown: ShownMachine }) {
  const { machine } = shown;
  const [mode, setMode] = useState<"show" | "rename" | "remove">("show");
  const [name, setName] = useState(machine.name);
  const reconnect = async () => report(await commands.reconnectMachine(machine.id));
  const rename = async () => {
    setMode("show");
    if (name.trim() !== machine.name) report(await commands.renameMachine(machine.id, name));
  };
  const remove = async () => report(await commands.removeMachine(machine.id));

  if (mode === "rename") {
    return (
      <div className={settings.row}>
        <input
          className={`${settings.field} ${styles.grow}`}
          aria-label={`Name of ${machine.destination}`}
          placeholder={machine.destination}
          value={name}
          spellCheck={false}
          autoFocus
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void rename();
            if (event.key === "Escape") {
              event.stopPropagation();
              setMode("show");
            }
          }}
        />
        <button className="button" onClick={() => setMode("show")}>
          Cancel
        </button>
        <button className="button accent" onClick={() => void rename()}>
          Rename
        </button>
      </div>
    );
  }
  if (mode === "remove") {
    return (
      <div className={settings.row} role="group" aria-label={`Remove ${machine.name}?`}>
        <div className={settings.rowText}>
          <span>Remove “{machine.name}”?</span>
          <span className={settings.detail}>
            localhost stops showing it here and closes its forwarded ports. Nothing changes on {machine.name}: its projects
            and services stay as they are.
          </span>
        </div>
        <button className="button" autoFocus onClick={() => setMode("show")}>
          Cancel
        </button>
        <button className="button destructive" onClick={() => void remove()}>
          Remove
        </button>
      </div>
    );
  }
  return (
    <div className={settings.row} role="group" aria-label={machine.name}>
      <div className={settings.rowText}>
        <span>
          {machine.name}
          {machine.name !== machine.destination && <span className={`${styles.destination} mono`}>{machine.destination}</span>}
        </span>
        <span className={settings.detail}>{shown.summary}</span>
      </div>
      {shown.needsUser && (
        <button className="button compact" aria-label={`Reconnect ${machine.name}`} onClick={() => void reconnect()}>
          Reconnect
        </button>
      )}
      <Menu
        label={`More actions for ${machine.name}`}
        items={[
          { label: "Reconnect", onSelect: () => void reconnect() },
          {
            label: "Rename…",
            onSelect: () => {
              setName(machine.name);
              setMode("rename");
            },
          },
          "separator",
          { label: "Remove…", isDestructive: true, onSelect: () => setMode("remove") },
        ]}
      />
    </div>
  );
}

/**
 * Where `ssh` reaches the Machine (a host of `~/.ssh/config`, suggested, or
 * `user@host:port`) and what to call it. Add connects first, and only once
 * that works is the Machine kept; otherwise it says why, and what to do next.
 */
function AddMachine({ onDone }: { onDone: () => void }) {
  const [destination, setDestination] = useState("");
  const [name, setName] = useState("");
  const [hosts, setHosts] = useState<string[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const hostsId = useId();
  const hintId = useId();

  useEffect(() => {
    void commands.sshHosts().then(setHosts);
  }, []);

  const add = async () => {
    if (!destination.trim() || isConnecting) return;
    setIsConnecting(true);
    const result = await commands.addMachine(destination.trim(), name.trim() || null);
    setIsConnecting(false);
    if (result.status === "ok") onDone();
    else setProblem(result.error.message);
  };
  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Enter") void add();
    if (event.key === "Escape" && !isConnecting) {
      event.stopPropagation();
      onDone();
    }
  };

  return (
    <form
      className={styles.add}
      aria-label="Add Machine"
      onSubmit={(event) => {
        event.preventDefault();
        void add();
      }}
    >
      <label className={styles.label}>
        SSH host
        <input
          className={`${settings.field} mono ${styles.grow}`}
          list={hostsId}
          placeholder={hosts[0] ?? "devbox"}
          value={destination}
          spellCheck={false}
          autoComplete="off"
          autoFocus
          disabled={isConnecting}
          aria-describedby={hintId}
          onChange={(event) => {
            setDestination(event.target.value);
            setProblem(null);
          }}
          onKeyDown={onKeyDown}
        />
        <datalist id={hostsId}>
          {hosts.map((host) => (
            <option key={host} value={host} />
          ))}
        </datalist>
      </label>
      <p id={hintId} className={settings.detail}>
        A host from ~/.ssh/config, or user@host:port. localhost connects as ssh does, with your keys and settings.
      </p>
      <label className={styles.label}>
        Name
        <input
          className={`${settings.field} ${styles.grow}`}
          placeholder={destination.trim() || "optional"}
          value={name}
          spellCheck={false}
          autoComplete="off"
          disabled={isConnecting}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={onKeyDown}
        />
      </label>
      {problem && (
        <p className={styles.problem} role="alert">
          {problem}
        </p>
      )}
      <div className={styles.footer}>
        {isConnecting && <span className="secondary">Connecting to {destination.trim()}…</span>}
        <button type="button" className="button" disabled={isConnecting} onClick={onDone}>
          Cancel
        </button>
        <button type="submit" className="button accent" disabled={!destination.trim() || isConnecting}>
          {isConnecting ? "Connecting…" : "Add"}
        </button>
      </div>
    </form>
  );
}
