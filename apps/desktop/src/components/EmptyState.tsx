import type { ReactNode } from "react";
import styles from "./EmptyState.module.css";

type Props = {
  icon: ReactNode;
  title: string;
  description: string;
  /** Buttons under the description. */
  children?: ReactNode;
};

/** A column with nothing to show, after macOS's ContentUnavailableView: an icon, a title, a line and actions. */
export function EmptyState({ icon, title, description, children }: Props) {
  return (
    <div className={styles.empty}>
      <span className={styles.icon}>{icon}</span>
      <h2 className={styles.title}>{title}</h2>
      <p className={styles.description}>{description}</p>
      {children && <div className={styles.actions}>{children}</div>}
    </div>
  );
}
