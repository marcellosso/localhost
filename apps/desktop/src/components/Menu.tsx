import { useEffect, useId, useRef, useState } from "react";
import styles from "./Menu.module.css";

export type MenuItem = { label: string; onSelect: () => void } | "separator";

type Props = {
  /** What the button says to a screen reader, e.g. "More actions for My SaaS". */
  label: string;
  items: MenuItem[];
};

/** A `⋯` button opening a menu of rare actions. ↑/↓ move, ↩ runs, Escape closes. */
export function Menu({ label, items }: Props) {
  const [isOpen, setIsOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menuId = useId();

  useEffect(() => {
    if (!isOpen) return;
    root.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    const closeOutside = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setIsOpen(false);
    };
    document.addEventListener("mousedown", closeOutside);
    return () => document.removeEventListener("mousedown", closeOutside);
  }, [isOpen]);

  const close = () => {
    setIsOpen(false);
    button.current?.focus();
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const entries = Array.from(root.current?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? []);
    const index = entries.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      event.stopPropagation();
      close();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      entries[(index + step + entries.length) % entries.length]?.focus();
    } else if (event.key === "Tab") {
      setIsOpen(false);
    }
  };

  return (
    <span ref={root} className={styles.root} onKeyDown={isOpen ? onKeyDown : undefined}>
      <button
        ref={button}
        className={`button ${styles.button}`}
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={isOpen}
        aria-controls={isOpen ? menuId : undefined}
        onClick={(event) => {
          event.stopPropagation();
          setIsOpen(!isOpen);
        }}
      >
        ⋯
      </button>
      {isOpen && (
        <ul id={menuId} className={styles.menu} role="menu" aria-label={label}>
          {items.map((item, index) =>
            item === "separator" ? (
              <li key={index} role="separator" className={styles.separator} />
            ) : (
              <li key={index} role="none">
                <button
                  role="menuitem"
                  tabIndex={-1}
                  onClick={(event) => {
                    event.stopPropagation();
                    close();
                    item.onSelect();
                  }}
                >
                  {item.label}
                </button>
              </li>
            ),
          )}
        </ul>
      )}
    </span>
  );
}
