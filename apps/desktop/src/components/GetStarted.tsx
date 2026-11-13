import { useEffect, useState } from "react";
import { commands, type Id } from "../bindings";
import type { CoreView } from "../core";
import {
  CHECKLIST_TITLE,
  type ChecklistItem,
  checklistItems,
  hasRunningService,
  isAnyAgentConnected,
  isChecklistShown,
  updateOnboarding,
  useOnboarding,
} from "../onboarding";
import { showSheet, useUi } from "../ui";
import { CheckIcon, ClearIcon } from "./Icons";
import styles from "./GetStarted.module.css";

type Props = {
  view: CoreView;
  /** The Project shown in the window, where Add a service goes first. */
  selectedProjectId: Id | null;
  onSelectProject: (projectId: Id) => void;
};

/**
 * The Get started checklist, at the foot of the sidebar once there is a
 * Project: each row ticks from what is true, an open row takes the user
 * there. It goes away for good when every row is ticked or it is closed.
 */
export function GetStarted({ view, selectedProjectId, onSelectProject }: Props) {
  const onboarding = useOnboarding();
  const isShown = onboarding !== null && isChecklistShown(onboarding, view);
  const hasConnectedAgent = useHasConnectedAgent(isShown);
  const isRunning = hasRunningService(view);
  const items = onboarding ? checklistItems(view, onboarding, hasConnectedAgent) : [];
  const isComplete = items.length > 0 && items.every((item) => item.isDone);

  useEffect(() => {
    if (isShown && isRunning && !onboarding?.hasStartedService) updateOnboarding({ hasStartedService: true });
  }, [isShown, isRunning, onboarding]);
  useEffect(() => {
    if (isShown && isComplete) updateOnboarding({ isChecklistDone: true });
  }, [isShown, isComplete]);

  if (!isShown || isComplete) return null;
  const doneCount = items.filter((item) => item.isDone).length;
  const act = (item: ChecklistItem) => {
    if (item.step === "agents") {
      showSheet({ kind: "connectAgents" });
      return;
    }
    const selected = view.projects.find((project) => project.id === selectedProjectId);
    if (item.step === "service") {
      const target = selected?.services.length === 0 ? selected : view.projects.find((project) => project.services.length === 0);
      if (target) showSheet({ kind: "detectServices", projectId: target.id });
    } else if (item.step === "start") {
      const target = selected?.services.length ? selected : view.projects.find((project) => project.services.length > 0);
      if (target) onSelectProject(target.id);
    }
  };

  return (
    <section className={styles.checklist} aria-label={CHECKLIST_TITLE}>
      <header className={styles.header}>
        <h2 className={styles.title}>{CHECKLIST_TITLE}</h2>
        <span className={`${styles.count} secondary`}>
          {doneCount} of {items.length}
        </span>
        <button
          className={styles.close}
          aria-label="Hide Get Started"
          title="Hide Get Started"
          onClick={() => updateOnboarding({ isChecklistDone: true })}
        >
          <ClearIcon size={13} />
        </button>
      </header>
      <ul className={styles.items}>
        {items.map((item) => (
          <li key={item.step} className={item.isDone ? styles.done : undefined}>
            <span className={styles.mark} aria-hidden="true">
              {item.isDone && <CheckIcon size={9} />}
            </span>
            {item.isDone ? (
              <span className={styles.label}>
                {item.title}
                <span className="visuallyHidden">, done</span>
              </span>
            ) : (
              <button className={styles.action} onClick={() => act(item)}>
                {item.title}
                {item.isOptional && (
                  <>
                    {" "}
                    <span className={`${styles.optional} secondary`}>(optional)</span>
                  </>
                )}
              </button>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

/**
 * Whether any agent has localhost's server, read while the checklist shows
 * and again each time a sheet closes, as Connect your agents is one.
 */
function useHasConnectedAgent(isShown: boolean): boolean {
  const { sheet } = useUi();
  const [isConnected, setIsConnected] = useState(false);
  const isSheetOpen = sheet !== null;
  useEffect(() => {
    if (!isShown || isSheetOpen) return;
    let isCurrent = true;
    void commands.agentStatuses().then((agents) => isCurrent && setIsConnected(isAnyAgentConnected(agents)));
    return () => {
      isCurrent = false;
    };
  }, [isShown, isSheetOpen]);
  return isConnected;
}
