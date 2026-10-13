import {
  dismissUnreadableFiles,
  leaveOrphans,
  start,
  stopHolderAndStart,
  stopListener,
  stopOrphans,
} from "../actions";
import type { BlockedStart } from "../bindings";
import { type CoreView, findService } from "../core";
import { processDescription } from "../listeners";
import { formatList } from "../status";
import { type Alert, alertKey, dismissAlert, useUi } from "../ui";
import { Dialog } from "./Dialog";
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
      const { listener } = alert.details;
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
              <button className="button destructive" onClick={then(() => stopListener(listener))}>
                Stop
              </button>
            </>
          }
        >
          <p className="mono">{processDescription(listener)}</p>
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

    case "settings":
      return alert.forEditor ? (
        <ChooseEditorDialog onChosen={alert.forEditor} onClose={close} />
      ) : (
        <SettingsDialog onClose={close} />
      );
  }
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
