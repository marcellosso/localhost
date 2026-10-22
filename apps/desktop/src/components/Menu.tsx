import { type CSSProperties, type MouseEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { CheckIcon, EllipsisIcon } from "./Icons";
import styles from "./Menu.module.css";

export type MenuItem =
  /** `isChecked` makes it one of a set of choices, with a checkmark on the chosen one. */
  | { label: string; onSelect: () => void; isChecked?: boolean; isDestructive?: boolean }
  | "separator";

type Props = {
  /** What the button says to a screen reader, e.g. "More actions for My SaaS". */
  label: string;
  items: MenuItem[];
  /** The button's content and class; a borderless `⋯` by default. */
  button?: { content: ReactNode; className: string };
  /** Which side of the button the menu lines up with. */
  align?: "left" | "right";
};

/** A button opening a menu: by default a borderless `⋯`, for rare actions. */
export function Menu({ label, items, button: custom, align = "right" }: Props) {
  const [isOpen, setIsOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  return (
    <span className={styles.root}>
      <button
        ref={button}
        className={custom?.className ?? "iconButton"}
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
        {custom?.content ?? <EllipsisIcon size={15} />}
      </button>
      {isOpen && (
        <MenuList
          id={menuId}
          label={label}
          items={items}
          className={`${styles.below} ${align === "left" ? styles.alignLeft : ""}`}
          onClose={(returnFocus) => {
            setIsOpen(false);
            if (returnFocus) button.current?.focus();
          }}
        />
      )}
    </span>
  );
}

/**
 * The same menu opened where the pointer is, for a right click. Spread
 * `onContextMenu` on the element and render `menu`.
 */
export function useContextMenu(
  label: string,
  items: () => MenuItem[],
): { onContextMenu: (event: MouseEvent) => void; menu: ReactNode } {
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null);
  return {
    onContextMenu: (event) => {
      event.preventDefault();
      setPosition({ x: event.clientX, y: event.clientY });
    },
    menu: position && (
      <MenuList
        label={label}
        items={items()}
        className={styles.atPointer}
        style={{ left: position.x, top: position.y }}
        onClose={() => setPosition(null)}
      />
    ),
  };
}

type ListProps = {
  id?: string;
  label: string;
  items: MenuItem[];
  className: string;
  style?: CSSProperties;
  /** `true` when closed from the keyboard, so focus can go back. */
  onClose: (returnFocus: boolean) => void;
};

/** ↑/↓ move, ↩ runs, Escape closes; a click elsewhere closes it too. */
function MenuList({ id, label, items, className, style, onClose }: ListProps) {
  const list = useRef<HTMLUListElement>(null);
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    list.current?.querySelector<HTMLElement>("[role^=menuitem]")?.focus();
    const closeOutside = (event: globalThis.MouseEvent) => {
      if (!list.current?.contains(event.target as Node)) close.current(false);
    };
    document.addEventListener("mousedown", closeOutside);
    return () => document.removeEventListener("mousedown", closeOutside);
  }, []);

  const onKeyDown = (event: React.KeyboardEvent) => {
    const entries = Array.from(list.current?.querySelectorAll<HTMLElement>("[role^=menuitem]") ?? []);
    const index = entries.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose(true);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      entries[(index + step + entries.length) % entries.length]?.focus();
    } else if (event.key === "Tab") {
      onClose(false);
    }
  };

  return (
    <ul
      ref={list}
      id={id}
      className={`${styles.menu} ${className}`}
      style={style}
      role="menu"
      aria-label={label}
      onKeyDown={onKeyDown}
      onClick={(event) => event.stopPropagation()}
    >
      {items.map((item, index) =>
        item === "separator" ? (
          <li key={index} role="separator" className={styles.separator} />
        ) : (
          <li key={index} role="none">
            <button
              role={item.isChecked === undefined ? "menuitem" : "menuitemradio"}
              aria-checked={item.isChecked}
              className={item.isDestructive ? styles.destructive : undefined}
              tabIndex={-1}
              onClick={() => {
                onClose(true);
                item.onSelect();
              }}
            >
              {item.isChecked !== undefined && (
                <span className={styles.check} aria-hidden="true">
                  {item.isChecked && <CheckIcon size={10} />}
                </span>
              )}
              {item.label}
            </button>
          </li>
        ),
      )}
    </ul>
  );
}
