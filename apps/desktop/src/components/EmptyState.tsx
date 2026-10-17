import type { ReactNode } from "react";
import styles from "./EmptyState.module.css";

type Props = {
  icon: ReactNode;
  title: string;
  description: string;
  /** Buttons under the description. */
  children?: ReactNode;
  /** A line under the buttons that teaches a shortcut, e.g. a `KeyTip`. */
  tip?: ReactNode;
};

/** A column with nothing to show, after macOS's ContentUnavailableView: an icon, a title, a line and actions. */
export function EmptyState({ icon, title, description, children, tip }: Props) {
  return (
    <div className={styles.empty}>
      <span className={styles.icon}>{icon}</span>
      <h2 className={styles.title}>{title}</h2>
      <p className={styles.description}>{description}</p>
      {children && <div className={styles.actions}>{children}</div>}
      {tip}
    </div>
  );
}

/** A line that names a shortcut in a key cap, e.g. "or press Ctrl+R", so an empty state teaches the keyboard too. */
export function KeyTip({ before, keys, after }: { before: string; keys: string; after?: string }) {
  return (
    <p className={`${styles.tip} secondary`}>
      {before} <kbd className="keyHint">{keys}</kbd>
      {after && ` ${after}`}
    </p>
  );
}
