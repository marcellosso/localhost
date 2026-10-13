import { type ReactNode, useEffect, useId, useRef } from "react";
import styles from "./Dialog.module.css";

type Props = {
  title: string;
  /** The message or form under the title. */
  children?: ReactNode;
  /** The buttons, the most likely one last. The one marked `data-autofocus` gets focus. */
  actions: ReactNode;
  /** Escape, as Cancel. */
  onCancel: () => void;
  /** `alertdialog` for questions and reports, `dialog` for forms. */
  role?: "alertdialog" | "dialog";
};

/** A modal over the main window. Focus stays inside it until it closes, then goes back. */
export function Dialog({ title, children, actions, onCancel, role = "alertdialog" }: Props) {
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const bodyId = useId();

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const element = panel.current;
    const first = element?.querySelector<HTMLElement>("[data-autofocus]") ?? focusable(element)[0];
    first?.focus();
    return () => previous?.focus();
  }, []);

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      onCancel();
    } else if (event.key === "Tab") {
      const elements = focusable(panel.current);
      if (elements.length === 0) return;
      const index = elements.indexOf(document.activeElement as HTMLElement);
      const next = event.shiftKey ? (index <= 0 ? elements.length - 1 : index - 1) : (index + 1) % elements.length;
      event.preventDefault();
      elements[next].focus();
    }
  };

  return (
    <div className={styles.backdrop}>
      <div
        ref={panel}
        className={styles.panel}
        role={role}
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={children ? bodyId : undefined}
        onKeyDown={onKeyDown}
      >
        <h2 id={titleId} className={styles.title}>
          {title}
        </h2>
        {children && (
          <div id={bodyId} className={styles.body}>
            {children}
          </div>
        )}
        <div className={styles.actions}>{actions}</div>
      </div>
    </div>
  );
}

function focusable(element: HTMLElement | null): HTMLElement[] {
  if (!element) return [];
  const selector = "button:not(:disabled), input:not(:disabled), select, textarea, [tabindex]:not([tabindex='-1'])";
  return Array.from(element.querySelectorAll<HTMLElement>(selector));
}
