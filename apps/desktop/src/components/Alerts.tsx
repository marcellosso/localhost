import {
  dismissUnreadableFiles,
  leaveOrphans,
  start,
  stopHolderAndStart,
  stopListener,
  stopOrphans,
} from "../actions";
import type { BlockedStart } from "../bindings";
import { removeProject, removeService } from "../editors";
import { type CoreView, findService } from "../core";
import { processDescription } from "../listeners";
import { formatList } from "../status";
import { isNativeWindows } from "../targets";
import { type Alert, alertKey, dismissAlert, useUi } from "../ui";
import { Dialog } from "./Dialog";
import { KeyboardShortcutsDialog } from "./KeyboardShortcutsDialog";
import { ChooseEditorDialog, SettingsDialog } from "./SettingsDialog";

/** The alert on top of the queue, and the progress note while something slow runs. */
export function Alerts({ view }: { view: CoreView }) {
  const { alerts, progress } = useUi();
  const alert = alerts[0];
  return (
    <>
      {alert && <AlertDialog key={alertKey(alert)} alert={alert} view={view} />}
      {progress && (
        <div className="progress" role="status">
          {progress}
        </div>
      )}
    </>
  );
}

function AlertDialog({ alert, view }: { alert: Alert; view: CoreView }) {
  const close = () => dismissAlert(alert);
  /** Closes the alert, then does `action`. */
  const then = (action: () => unknown) => () => {
    close();
    void action();
  };

  switch (alert.kind) {
    case "error":
      return (
        <Dialog
          title="Something went wrong"
          onCancel={close}
          actions={
            <button className="button accent" onClick={close} data-autofocus>
              OK
            </button>
          }
        >
          <p>{alert.message}</p>
        </Dialog>
      );

    case "blockedStart": {
      const { blocked } = alert;
      const found = findService(view, blocked.serviceId);
      const name = found?.project.services[found.index].name ?? "The service";
      const holder = blocked.holder;
      const canStopHolder = holder !== null && holder.reasonNotStoppable === null;
      return (
        <Dialog
          title={`${name} can’t start: port ${blocked.port} is in use`}
          onCancel={close}
          actions={
            <>
              <button className="button" onClick={close} data-autofocus>
                Cancel
              </button>
              {blocked.nextFreePort !== null && (
                <button className="button" onClick={then(() => start(blocked.serviceId, blocked.nextFreePort))}>
                  Start on {blocked.nextFreePort}
                </button>
              )}
              {canStopHolder && (
                <button className="button accent" onClick={then(() => stopHolderAndStart(blocked))}>
                  Stop It and Start
                </button>
              )}
            </>
          }
        >
          {blockedStartMessage(blocked).map((paragraph) => (
            <p key={paragraph}>{paragraph}</p>
          ))}
        </Dialog>
      );
    }

    case "stopListener": {
      const { details } = alert;
      const { listener } = details;
      const ports = listener.ports.join(", ");
      return (
        <Dialog
          title={`Stop ${listener.name} on port${listener.ports.length === 1 ? "" : "s"} ${ports}?`}
          onCancel={close}
          actions={
            <>
              <button className="button" onClick={close} data-autofocus>
                Cancel
              </button>
              <button className="button destructive" onClick={then(() => stopListener(details))}>
                Stop
              </button>
            </>
          }
        >
          <p className="mono">{processDescription(listener)}</p>
          {isNativeWindows(details.target) && (
            <p>Windows can’t ask it to quit: it and the processes it started end at once, without saving anything.</p>
          )}
        </Dialog>
      );
    }

    case "unreadableFiles": {
      const count = alert.files.length;
      return (
        <Dialog
          title={count === 1 ? "1 project couldn’t be loaded" : `${count} projects couldn’t be loaded`}
          onCancel={then(() => dismissUnreadableFiles(false))}
          actions={
            <>
              <button className="button" onClick={then(() => dismissUnreadableFiles(true))}>
                Show in File Manager
              </button>
              <button className="button accent" onClick={then(() => dismissUnreadableFiles(false))} data-autofocus>
                OK
              </button>
            </>
          }
        >
          {alert.files.map((file) => (
            <p key={file}>{file}</p>
          ))}
        </Dialog>
      );
    }

    case "orphans": {
      const count = alert.orphans.length;
      return (
        <Dialog
          title={
            count === 1
              ? "1 service from the last session is still running"
              : `${count} services from the last session are still running`
          }
          onCancel={then(leaveOrphans)}
          actions={
            <>
              <button className="button" onClick={then(leaveOrphans)}>
                Leave Them
              </button>
              <button className="button accent" onClick={then(stopOrphans)} data-autofocus>
                Stop Them
              </button>
            </>
          }
        >
          <p>{alert.orphans.join("\n")}</p>
        </Dialog>
      );
    }

    case "removeProject":
    case "removeService": {
      const name =
        alert.kind === "removeProject"
          ? view.projects.find((project) => project.id === alert.projectId)?.name
          : (() => {
              const found = findService(view, alert.serviceId);
              return found?.project.services[found.index].name;
            })();
      const remove = alert.kind === "removeProject" ? () => removeProject(alert.projectId) : () => removeService(alert.serviceId);
      return (
        <Dialog
          title={`Remove “${name ?? (alert.kind === "removeProject" ? "project" : "service")}”?`}
          onCancel={close}
          actions={
            <>
              <button className="button" onClick={close} data-autofocus>
                Cancel
              </button>
              <button className="button destructive" onClick={then(remove)}>
                Remove
              </button>
            </>
          }
        >
          {removalMessage(alert).map((paragraph) => (
            <p key={paragraph}>{paragraph}</p>
          ))}
        </Dialog>
      );
    }

    case "importFailure":
      return (
        <Dialog
          title={`“${alert.fileName}” can’t be imported`}
          onCancel={close}
          actions={
            <button className="button accent" onClick={close} data-autofocus>
              OK
            </button>
          }
        >
          <p>{alert.message}</p>
        </Dialog>
      );

    case "settings":
      return alert.forEditor ? (
        <ChooseEditorDialog onChosen={alert.forEditor} onClose={close} />
      ) : (
        <SettingsDialog onClose={close} />
      );

    case "keyboardShortcuts":
      return <KeyboardShortcutsDialog onClose={close} />;
  }
}

/** What removing deletes, and which Services stop depending on a removed one. */
export function removalMessage(alert: Extract<Alert, { kind: "removeProject" | "removeService" }>): string[] {
  const paragraphs = [
    "Running services are stopped first. Their configuration is deleted; the project folder is not touched.",
  ];
  if (alert.kind === "removeService" && alert.dependents.length > 0) {
    paragraphs.push(`${formatList(alert.dependents.map((name) => `“${name}”`))} will no longer depend on it.`);
  }
  return paragraphs;
}

/** Who holds the port, why it can't be stopped, and what starting elsewhere means. */
export function blockedStartMessage(blocked: BlockedStart): string[] {
  const paragraphs: string[] = [];
  const holder = blocked.holder;
  if (holder) {
    paragraphs.push(`It is held by ${processDescription(holder.listener)}\n(${holder.owner}).`);
    if (holder.reasonNotStoppable) paragraphs.push(holder.reasonNotStoppable);
  } else {
    paragraphs.push("Another process, which localhost can’t inspect, is listening on it.");
  }
  if (blocked.nextFreePort === null) {
    paragraphs.push("No free port was found within 100 ports above it.");
  } else if (blocked.dependents.length > 0) {
    const names = formatList(blocked.dependents.map((name) => `“${name}”`));
    const verb = blocked.dependents.length === 1 ? "depends" : "depend";
    paragraphs.push(
      `Starting on ${blocked.nextFreePort}: ${names} ${verb} on it and may still expect port ${blocked.port}.`,
    );
  }
  return paragraphs;
}
