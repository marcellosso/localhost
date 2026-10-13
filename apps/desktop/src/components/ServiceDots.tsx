import styles from "./ServiceDots.module.css";
import { serviceColor } from "./ServicesColumn";

/** A Project's Services as a cluster of their colours (the first four); inactive ones faded. */
export function ServiceDots({ active }: { active: boolean[] }) {
  return (
    <span className={styles.dots} aria-hidden="true">
      {active.slice(0, 4).map((isActive, index) => (
        <span
          key={index}
          className={styles.dot}
          style={{ background: serviceColor(index), opacity: isActive ? 1 : 0.35 }}
        />
      ))}
    </span>
  );
}
