import { useEffect, useRef, useState } from "react";
import type { Id } from "../bindings";
import type { CoreView } from "../core";
import { currentStates, settledAnnouncements } from "../announce";

/** How long a message stays in its live region: long enough to be read, then out of the way. */
const MESSAGE_LIFETIME = 10_000;

type Message = { id: number; text: string };

/**
 * Two live regions, out of sight, that tell a screen reader when a Service
 * settles: politely, or at once for Failed. Each message is a new element,
 * so the same words twice are still spoken twice.
 */
export function Announcer({ view, selectedProjectId }: { view: CoreView; selectedProjectId: Id | null }) {
  const states = useRef<ReturnType<typeof currentStates> | null>(null);
  const nextId = useRef(0);
  const [polite, setPolite] = useState<Message[]>([]);
  const [urgent, setUrgent] = useState<Message[]>([]);

  useEffect(() => {
    if (!states.current) {
      states.current = currentStates(view);
      return;
    }
    for (const { text, isUrgent } of settledAnnouncements(states.current, view, selectedProjectId)) {
      const message = { id: nextId.current++, text };
      const setMessages = isUrgent ? setUrgent : setPolite;
      setMessages((messages) => [...messages, message]);
      setTimeout(() => setMessages((messages) => messages.filter((other) => other !== message)), MESSAGE_LIFETIME);
    }
    // Only new statuses are announced; the selection is read as they arrive.
  }, [view.statuses]);

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
