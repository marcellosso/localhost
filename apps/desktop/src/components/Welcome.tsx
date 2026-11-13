import { type CSSProperties, Fragment, useId, useRef, useState } from "react";
import { announce } from "../announce";
import { updateOnboarding, WELCOME_STEPS, WELCOME_TITLE } from "../onboarding";
import { closeSheet, showSheet } from "../ui";
import { useModal } from "./Dialog";
import { FolderIcon } from "./Icons";
import { serviceColor } from "./ServicesColumn";
import { StateGlyph } from "./StateGlyph";
import styles from "./Welcome.module.css";

/**
 * The first launch's welcome: what a Project, a Service and a Port are, one
 * step each, ending in Add Project…. Skipping, Escape or finishing all
 * mean it never opens by itself again; Help › Show Welcome… opens it anyway.
 */
export function Welcome() {
  const [index, setIndex] = useState(0);
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const bodyId = useId();
  const primary = useRef<HTMLButtonElement>(null);
  const step = WELCOME_STEPS[index];
  const isLast = index === WELCOME_STEPS.length - 1;
  // Focus stays on the step's main button, and a screen reader hears the new step.
  const go = (next: number) => {
    setIndex(next);
    announce(`Step ${next + 1} of ${WELCOME_STEPS.length}, ${WELCOME_STEPS[next].title}`);
    requestAnimationFrame(() => primary.current?.focus());
  };
  const finish = (addProject: boolean) => {
    updateOnboarding({ hasSeenWelcome: true });
    if (addProject) showSheet({ kind: "addProject" });
    else closeSheet();
  };
  const onKeyDown = useModal(panel, () => finish(false));

  return (
    <div className={styles.backdrop}>
      <div
        ref={panel}
        className={styles.panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        onKeyDown={onKeyDown}
      >
        <div className={styles.picture} aria-hidden="true">
          <Picture index={index} />
        </div>
        <div className={styles.text}>
          <p className={styles.caption}>
            {WELCOME_TITLE}
            <span className="visuallyHidden">
              , step {index + 1} of {WELCOME_STEPS.length}
            </span>
          </p>
          <h2 id={titleId} className={styles.title}>
            {step.title}
          </h2>
          <p id={bodyId} className={`${styles.body} secondary`}>
            <WithCode text={step.body} />
          </p>
        </div>
        <footer className={styles.footer}>
          <button className={styles.skip} onClick={() => finish(false)}>
            {isLast ? "Not Now" : "Skip"}
          </button>
          <span className={styles.dots} aria-hidden="true">
            {WELCOME_STEPS.map((_, dot) => (
              <span key={dot} className={dot === index ? styles.current : undefined} />
            ))}
          </span>
          <span className={styles.buttons}>
            {index > 0 && (
              <button className="button" onClick={() => go(index - 1)}>
                Back
              </button>
            )}
            <button
              ref={primary}
              className="button accent"
              onClick={() => (isLast ? finish(true) : go(index + 1))}
              data-autofocus
            >
              {isLast ? "Add Project…" : "Continue"}
            </button>
          </span>
        </footer>
      </div>
    </div>
  );
}

/** A step's words, its backticked commands in monospace. */
function WithCode({ text }: { text: string }) {
  return text.split("`").map((part, index) =>
    index % 2 === 1 ? (
      <code key={index} className={`${styles.code} mono`}>
        {part}
      </code>
    ) : (
      <Fragment key={index}>{part}</Fragment>
    ),
  );
}

/** A small mock of the window for each step: the folder, its Services, then a Port that answers. */
function Picture({ index }: { index: number }) {
  if (index === 0) {
    return (
      <div className={styles.card}>
        <div className={styles.folder}>
          <FolderIcon size={18} />
          <span className="mono">~/code/my-saas</span>
        </div>
        <div className={styles.arrow} />
        <div className={`${styles.row} ${styles.selected}`}>
          <StateGlyph state={null} />
          <span className={styles.name}>My SaaS</span>
        </div>
        <div className={styles.row}>
          <StateGlyph state={null} />
          <span className={styles.name}>Blog</span>
        </div>
      </div>
    );
  }
  const rows =
    index === 1
      ? [
          { name: "Web", command: "pnpm dev", state: "stopped" as const, port: null },
          { name: "Database", command: "docker compose up", state: "stopped" as const, port: null },
        ]
      : [
          { name: "Web", command: "pnpm dev", state: "running" as const, port: 3000 },
          { name: "Database", command: "docker compose up", state: "running" as const, port: 5432 },
        ];
  return (
    <div className={styles.card}>
      {rows.map((row, position) => (
        <div key={row.name} className={styles.service} style={{ "--service": serviceColor(position) } as CSSProperties}>
          <StateGlyph state={row.state} />
          <span className={styles.serviceText}>
            <span className={styles.name}>{row.name}</span>
            <span className={`${styles.command} mono secondary`}>{row.command}</span>
          </span>
          {row.port !== null && <span className="portPill">:{row.port}</span>}
          {index === 2 && <span className={styles.running}>Running</span>}
        </div>
      ))}
    </div>
  );
}
