import { useId, useRef } from "react";
import { isShortcut, SHORTCUTS } from "../keyboard";
import { BADGE_MODIFIERS, shortcutGuide } from "../shortcutGuide";
import { useModal } from "./Dialog";
import { KeyboardIcon } from "./Icons";
import styles from "./KeyboardShortcutsDialog.module.css";

/**
 * Every keyboard shortcut, grouped (Ctrl+/, the sidebar's keyboard button or
 * the palette), after the macOS app's Keyboard Shortcuts sheet. Escape,
 * Done or Ctrl+/ again closes it.
 */
export function KeyboardShortcutsDialog({ onClose }: { onClose: () => void }) {
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const onModalKeyDown = useModal(panel, onClose);
  const [hold, holdMore] = BADGE_MODIFIERS;

  return (
    <div className={styles.backdrop}>
      <div
        ref={panel}
        className={styles.panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={(event) => {
          if (isShortcut(event.nativeEvent, SHORTCUTS.keyboardShortcuts)) {
            event.preventDefault();
            onClose();
          } else onModalKeyDown(event);
        }}
      >
        <header className={styles.header}>
          <span className={styles.icon}>
            <KeyboardIcon size={16} />
          </span>
          <h2 id={titleId}>Keyboard Shortcuts</h2>
        </header>
        <div className={styles.columns}>
          {shortcutGuide().map((groups, index) => (
            <div key={index} className={styles.column}>
              {groups.map((group) => (
                <section key={group.title} aria-label={group.title}>
                  <h3 className={styles.group}>{group.title}</h3>
                  <dl className={styles.entries}>
                    {group.entries.map((entry) => (
                      <div key={entry.title} className={styles.entry}>
                        <dt>{entry.title}</dt>
                        <dd>
                          {entry.keys.map((key) => (
                            <kbd key={key} className="keyHint">
                              {key}
                            </kbd>
                          ))}
                        </dd>
                      </div>
                    ))}
                  </dl>
                </section>
              ))}
            </div>
          ))}
        </div>
        <footer className={styles.footer}>
          <p className="secondary">
            Hold <kbd className="keyHint">{hold}</kbd> or <kbd className="keyHint">{holdMore}</kbd> to see each row’s
            number.
          </p>
          <button className="button accent" onClick={onClose} data-autofocus>
            Done
          </button>
        </footer>
      </div>
    </div>
  );
}
