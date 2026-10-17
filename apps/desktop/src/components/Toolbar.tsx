import type { ReactNode } from "react";
import styles from "./Toolbar.module.css";

type Props = {
  title: string;
  subtitle?: string;
  /** Over the log, right-aligned. */
  actions?: ReactNode;
};

/**
 * The window's toolbar: one strip across the Services column and the log,
 * its title over the column and its actions over the log.
 */
export function Toolbar({ title, subtitle, actions }: Props) {
  return (
    <>
      <div className={styles.title}>
        <h1>{title}</h1>
        {subtitle && <span className="secondary">{subtitle}</span>}
      </div>
      <div className={styles.actions}>{actions}</div>
    </>
  );
}
