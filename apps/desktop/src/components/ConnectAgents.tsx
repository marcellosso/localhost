import { useEffect, useId, useRef, useState } from "react";
import { announce } from "../announce";
import { type AgentConnection, type AgentStatus, type CommandLineStatus, commands } from "../bindings";
import { tildePath } from "../paths";
import { showAlert } from "../ui";
import { useModal } from "./Dialog";
import { ChevronIcon, LinkIcon } from "./Icons";
import styles from "./ConnectAgents.module.css";

export const CONNECT_AGENTS_TITLE = "Connect your agents";
export const CONNECT_AGENTS_SUBTITLE =
  "Let Claude Code, Cursor and Codex start, stop and read your Services through localhost. Nothing changes until you click Connect.";

const MANUAL_SETUP_URL = "https://www.getlocalhost.app/docs/mcp";

const STATE_LABELS: Record<AgentConnection, string> = {
  connected: "Connected",
  notConnected: "Not connected",
  needsUpdate: "Update available",
  notInstalled: "Not installed",
  unreadable: "Can’t edit",
};

/**
 * Connects Claude Code, Cursor and Codex to localhost's MCP server, and puts
 * the `localhost` command on the terminal's PATH. Every sentence about where
 * an agent stands comes from the core. The agents show first: finding the
 * command asks the login shell, which can take a few seconds.
 */
export function ConnectAgents({ headingLevel = 3 }: { headingLevel?: 3 | 4 }) {
  const [agents, setAgents] = useState<AgentStatus[] | null>(null);
  const [commandLine, setCommandLine] = useState<CommandLineStatus | null>(null);

  useEffect(() => {
    let isShown = true;
    void commands.agentStatuses().then((statuses) => isShown && setAgents(statuses));
    void commands.commandLineStatus().then((status) => isShown && setCommandLine(status));
    return () => {
      isShown = false;
    };
  }, []);

  const replace = (status: AgentStatus) =>
    setAgents((agents) => agents?.map((agent) => (agent.client === status.client ? status : agent)) ?? null);

  return (
    <div className={styles.agents}>
      {agents ? (
        <div className={styles.list}>
          {agents.map((agent) => (
            <AgentRow key={agent.client} agent={agent} onChange={replace} />
          ))}
        </div>
      ) : (
        <p className={`${styles.waiting} secondary`}>Looking for agents…</p>
      )}
      <CommandLineSection status={commandLine} onChange={setCommandLine} headingLevel={headingLevel} />
      <button className={styles.link} onClick={() => void openManualSetup()}>
        Set up by hand
      </button>
    </div>
  );
}

function AgentRow({ agent, onChange }: { agent: AgentStatus; onChange: (status: AgentStatus) => void }) {
  const [isWorking, setIsWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isShowingChanges, setIsShowingChanges] = useState(false);
  const nameId = useId();
  const summaryId = useId();
  const changesId = useId();

  const run = async (action: "connect" | "disconnect") => {
    setIsWorking(true);
    setError(null);
    const result = await (action === "connect" ? commands.connectAgent : commands.disconnectAgent)(agent.client);
    setIsWorking(false);
    if (result.status === "ok") {
      onChange(result.data);
      announce(`${result.data.name}, ${STATE_LABELS[result.data.connection]}`);
    } else {
      setError(result.error.message);
    }
  };

  const action = (() => {
    switch (agent.connection) {
      case "notConnected":
        return { title: "Connect", className: "button accent", run: () => run("connect") };
      case "needsUpdate":
        return { title: "Update", className: "button accent", run: () => run("connect") };
      case "connected":
        return { title: "Disconnect", className: "button", run: () => run("disconnect") };
      default:
        return null;
    }
  })();

  return (
    <div className={styles.row} role="group" aria-labelledby={nameId} aria-describedby={summaryId}>
      <div className={styles.rowMain}>
        <div className={styles.rowText}>
          <div className={styles.titleLine}>
            <span id={nameId} className={styles.name}>
              {agent.name}
            </span>
            <span className={styles.state}>{STATE_LABELS[agent.connection]}</span>
          </div>
          <p id={summaryId} className={`${styles.summary} secondary`}>
            <CoreText text={agent.summary} />
          </p>
        </div>
        {action && (
          <button
            className={action.className}
            aria-label={`${action.title} ${agent.name}`}
            disabled={isWorking}
            onClick={() => void action.run()}
          >
            {action.title}
          </button>
        )}
      </div>
      {error && (
        <p className={styles.error} role="alert">
          <CoreText text={error} />
        </p>
      )}
      {action && (
        <>
          <button
            className={`${styles.disclosure} secondary`}
            aria-expanded={isShowingChanges}
            aria-controls={changesId}
            aria-label={`What changes for ${agent.name}`}
            onClick={() => setIsShowingChanges(!isShowingChanges)}
          >
            <ChevronIcon size={9} className={styles.chevron} />
            What changes
          </button>
          {isShowingChanges && (
            <div id={changesId} className={styles.changes}>
              <p>
                Adds to <span className="mono">{tildePath(agent.configPath)}</span>:
              </p>
              <pre className={`${styles.entry} mono`}>{agent.entry}</pre>
              <p>
                Installs the skill at <span className="mono">{tildePath(agent.skillPath)}</span>
              </p>
              {agent.configuredCommand && agent.connection === "needsUpdate" && (
                <p>
                  Now runs <span className="mono">{agent.configuredCommand}</span>
                </p>
              )}
              {agent.backupPath && (
                <p>
                  Original saved at <span className="mono">{tildePath(agent.backupPath)}</span>
                </p>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}

type CommandLineProps = {
  status: CommandLineStatus | null;
  onChange: (status: CommandLineStatus) => void;
  headingLevel: 3 | 4;
};

/** Whether terminals find `localhost`, and adding or removing the link localhost made. */
function CommandLineSection({ status, onChange, headingLevel }: CommandLineProps) {
  const [isWorking, setIsWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const headingId = useId();
  const Heading = headingLevel === 3 ? "h3" : "h4";

  const run = async (action: "add" | "remove") => {
    setIsWorking(true);
    setError(null);
    const result = await (action === "add" ? commands.addCommandToPath : commands.removeCommandFromPath)();
    setIsWorking(false);
    if (result.status === "ok") {
      onChange(result.data);
      announce(result.data.summary);
    } else {
      setError(result.error.message);
    }
  };

  const action =
    status?.state === "notAdded"
      ? { title: "Add to PATH", run: () => run("add") }
      : status?.state === "added"
        ? { title: "Remove from PATH", run: () => run("remove") }
        : null;

  return (
    <section className={styles.terminal} aria-labelledby={headingId}>
      <Heading id={headingId} className={styles.caption}>
        Use localhost in a terminal
      </Heading>
      {status ? (
        <div className={styles.rowMain}>
          <div className={styles.rowText}>
            <p className={styles.summary}>
              <CoreText text={status.summary} />
            </p>
            <p className={`${styles.path} mono secondary`}>{tildePath(status.path)}</p>
          </div>
          {action && (
            <button className="button" disabled={isWorking} onClick={() => void action.run()}>
              {action.title}
            </button>
          )}
        </div>
      ) : (
        <p className={`${styles.waiting} secondary`}>Looking for the localhost command…</p>
      )}
      {error && (
        <p className={styles.error} role="alert">
          <CoreText text={error} />
        </p>
      )}
    </section>
  );
}

/** Text the core wrote, its `code` spans in monospace without the backticks. */
function CoreText({ text }: { text: string }) {
  return text.split("`").map((part, index) =>
    index % 2 === 1 ? (
      <code key={index} className="mono">
        {part}
      </code>
    ) : (
      part
    ),
  );
}

async function openManualSetup() {
  const result = await commands.openUrl(MANUAL_SETUP_URL);
  if (result.status === "error") showAlert({ kind: "error", message: result.error.message });
}

/** The same, over the main window (the palette, the empty state). Escape or Done closes it. */
export function ConnectAgentsSheet({ onClose }: { onClose: () => void }) {
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const subtitleId = useId();
  const onKeyDown = useModal(panel, onClose);

  return (
    <div className={styles.backdrop}>
      <div
        ref={panel}
        className={styles.panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={subtitleId}
        onKeyDown={onKeyDown}
      >
        <header className={styles.header}>
          <span className={styles.icon}>
            <LinkIcon size={16} />
          </span>
          <div className={styles.heading}>
            <h2 id={titleId}>{CONNECT_AGENTS_TITLE}</h2>
            <p id={subtitleId} className="secondary">
              {CONNECT_AGENTS_SUBTITLE}
            </p>
          </div>
        </header>
        <div className={styles.content}>
          <ConnectAgents />
        </div>
        <footer className={styles.footer}>
          <button className="button" onClick={onClose} data-autofocus>
            Done
          </button>
        </footer>
      </div>
    </div>
  );
}
