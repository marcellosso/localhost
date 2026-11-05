import { useEffect, useRef, useState } from "react";
import type { Id, Project, ServiceState } from "../bindings";
import type { CoreView } from "../core";
import { type Announcement, announce, currentStates, onAnnouncement, settledAnnouncements } from "../announce";
import type { ShownMachine } from "../machines";

/** How long a message stays in its live region: long enough to be read, then out of the way. */
const MESSAGE_LIFETIME = 10_000;

type Message = { id: number; text: string };

/**
 * Two live regions, out of sight, that tell a screen reader when a Service
 * settles, or what an `announce` reports: politely, or at once for Failed.
 * Each message is a new element, so the same words twice are still spoken twice.
 */
export function Announcer({ view, selectedProjectId }: { view: CoreView; selectedProjectId: Id | null }) {
  const states = useRef<ReturnType<typeof currentStates> | null>(null);
  const nextId = useRef(0);
  const [polite, setPolite] = useState<Message[]>([]);
  const [urgent, setUrgent] = useState<Message[]>([]);

  const say = ({ text, isUrgent }: Announcement) => {
    const message = { id: nextId.current++, text };
    const setMessages = isUrgent ? setUrgent : setPolite;
    setMessages((messages) => [...messages, message]);
    setTimeout(() => setMessages((messages) => messages.filter((other) => other !== message)), MESSAGE_LIFETIME);
  };

  useEffect(() => {
    if (!states.current) {
      states.current = currentStates(view);
      return;
    }
    settledAnnouncements(states.current, view, selectedProjectId).forEach(say);
    // Only new statuses are announced; the selection is read as they arrive.
  }, [view.statuses]);

  useEffect(() => onAnnouncement(say), []);

  return (
    <>
      <div className="visuallyHidden" role="status" aria-live="polite">
        {polite.map((message) => (
          <p key={message.id}>{message.text}</p>
        ))}
      </div>
      <div className="visuallyHidden" role="alert" aria-live="assertive">
        {urgent.map((message) => (
          <p key={message.id}>{message.text}</p>
        ))}
      </div>
    </>
  );
}

/**
 * Says when a Service of another Machine settles, through `Announcer`: with
 * the Machine's name unless it is the selected one. What a Machine reads
 * as it connects (its Projects afresh) is not announced.
 */
export function MachinesAnnouncer({
  machines,
  selected,
}: {
  machines: ShownMachine[];
  selected: { machineId: Id; projectId: Id | null } | null;
}) {
  const seen = useRef(new Map<Id, { projects: Project[]; states: Map<Id, ServiceState> }>());

  useEffect(() => {
    for (const shown of machines) {
      const id = shown.machine.id;
      const last = seen.current.get(id);
      if (!last || last.projects !== shown.view.projects) {
        seen.current.set(id, { projects: shown.view.projects, states: currentStates(shown.view) });
        continue;
      }
      const isSelected = selected?.machineId === id;
      for (const { text, isUrgent } of settledAnnouncements(last.states, shown.view, isSelected ? selected.projectId : null)) {
        announce(isSelected ? text : `${shown.machine.name}, ${text}`, isUrgent);
      }
    }
    // Only new statuses are announced; the selection is read as they arrive.
  }, [machines]);

  return null;
}
