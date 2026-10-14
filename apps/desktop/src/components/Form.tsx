import { type CSSProperties, type FormEvent, type ReactNode, useId, useRef, useState } from "react";
import { tildePath } from "../paths";
import { useModal } from "./Dialog";
import { CheckIcon, ChevronIcon, FolderIcon } from "./Icons";
import styles from "./Form.module.css";

// The pieces every editor sheet is built from, after the macOS FormKit: a
// header with an icon, labels above fields, boxed fields with an accent focus
// ring, cards, chips, switches, and a footer with one accent button.

type SheetProps = {
  icon: ReactNode;
  /** The icon's tint, e.g. a Service's colour; the accent by default. */
  tint?: string;
  title: string;
  subtitle?: ReactNode;
  /** Shown in the footer unless there is an error, e.g. "↩ to add". */
  hint?: string | null;
  error?: string | null;
  primaryTitle: string;
  isPrimaryDisabled?: boolean;
  width?: number;
  onCancel: () => void;
  /** ↩ in a field, or the primary button. */
  onSubmit: () => void;
  children: ReactNode;
};

/**
 * A sheet: icon, title and context, the fields (scrolling when long), then a
 * footer with a hint or the error, Cancel and the primary action.
 */
export function FormSheet(props: SheetProps) {
  const { icon, tint, title, subtitle, hint, error, primaryTitle, isPrimaryDisabled, width = 560 } = props;
  const panel = useRef<HTMLFormElement>(null);
  const titleId = useId();
  const onKeyDown = useModal(panel, props.onCancel);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!isPrimaryDisabled) props.onSubmit();
  };
  return (
    <div className={styles.backdrop}>
      <form
        ref={panel}
        className={styles.sheet}
        style={{ width: `min(${width}px, calc(100vw - 48px))`, "--tint": tint ?? "var(--accent)" } as CSSProperties}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={onKeyDown}
        onSubmit={submit}
      >
        <header className={styles.header}>
          <span className={styles.icon}>{icon}</span>
          <div className={styles.heading}>
            <h2 id={titleId}>{title}</h2>
            {subtitle && <p className="secondary">{subtitle}</p>}
          </div>
        </header>
        <div className={styles.content}>{props.children}</div>
        <footer className={styles.footer}>
          {error ? (
            <p className={styles.error} role="alert">
              {error}
            </p>
          ) : (
            <p className={`${styles.hint} secondary`}>{hint}</p>
          )}
          <button type="button" className="button" onClick={props.onCancel}>
            Cancel
          </button>
          <button type="submit" className="button accent" disabled={isPrimaryDisabled}>
            {primaryTitle}
          </button>
        </footer>
      </form>
    </div>
  );
}

type FieldProps = {
  title: string;
  /** A quiet note on the right of the label, e.g. "optional". */
  note?: string | null;
  hint?: string | null;
  /** The field's width; it grows to fill otherwise. */
  width?: number;
  /** Labels the control with `title` when it has a single input. */
  children: (id: string) => ReactNode;
};

/** A labelled field: the label above, then the control, then an optional hint. */
export function Field({ title, note, hint, width, children }: FieldProps) {
  const id = useId();
  return (
    <div className={styles.field} style={width ? { flex: `0 0 ${width}px` } : undefined}>
      <FieldLabel title={title} note={note} htmlFor={id} />
      {children(id)}
      {hint && <FieldHint>{hint}</FieldHint>}
    </div>
  );
}

export function FieldLabel({ title, note, htmlFor }: { title: string; note?: string | null; htmlFor?: string }) {
  return (
    <div className={styles.label}>
      <label htmlFor={htmlFor}>{title}</label>
      {note && <span className="secondary">{note}</span>}
    </div>
  );
}

export function FieldHint({ children }: { children: ReactNode }) {
  return <p className={`${styles.fieldHint} secondary`}>{children}</p>;
}

/** Fields that belong together, closer than the sheet's sections, e.g. Port and URL with their hint. */
export function FieldGroup({ children }: { children: ReactNode }) {
  return <div className={styles.group}>{children}</div>;
}

/** Fields side by side, e.g. Name and Key. */
export function FieldRow({ children }: { children: ReactNode }) {
  return <div className={styles.row}>{children}</div>;
}

type TextFieldProps = {
  id?: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  isMonospaced?: boolean;
  label?: string;
  autoFocus?: boolean;
};

/** A text field in a rounded box that shows the accent ring while focused. */
export function TextField({ id, value, onChange, placeholder, isMonospaced, label, autoFocus }: TextFieldProps) {
  return (
    <input
      id={id}
      className={`${styles.box} ${styles.input} ${isMonospaced ? "mono" : ""}`}
      value={value}
      placeholder={placeholder}
      aria-label={label}
      spellCheck={false}
      autoComplete="off"
      data-autofocus={autoFocus || undefined}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

/** A folder's path in a field box; choosing opens the folder picker. */
export function FolderField({ id, path, onChoose }: { id?: string; path: string; onChoose: () => void }) {
  return (
    <button type="button" id={id} className={`${styles.box} ${styles.folder}`} onClick={onChoose}>
      <FolderIcon size={14} className="secondary" />
      <span className={`${styles.folderPath} mono ${path ? "" : "secondary"}`}>
        {/* Truncated at the start, so the folder's name shows; `ltr` keeps the path's own order. */}
        <span dir="ltr">{path ? tildePath(path) : "Choose a folder…"}</span>
      </span>
      <span className={`${styles.choose} secondary`}>Choose…</span>
    </button>
  );
}

type CommandProps = {
  id?: string;
  workingDirectory: string;
  commandLine: string;
  onCommandLineChange: (commandLine: string) => void;
  onChooseDirectory: () => void;
};

/** The command as a terminal line: its working directory (click to choose), a `$`, then the command line. */
export function CommandField({ id, workingDirectory, commandLine, onCommandLineChange, onChooseDirectory }: CommandProps) {
  const isRoot = workingDirectory === "" || workingDirectory === ".";
  return (
    <div className={`${styles.box} ${styles.command}`}>
      <button
        type="button"
        className={`${styles.directory} mono secondary`}
        title="Working directory, relative to the project. Click to choose a folder."
        aria-label={`Working directory: ${isRoot ? "project folder" : workingDirectory}. Chooses a folder.`}
        onClick={onChooseDirectory}
      >
        <FolderIcon size={11} />
        <span className={styles.directoryPath}>
          <span dir="ltr">{isRoot ? "./" : `./${workingDirectory}`}</span>
        </span>
        <ChevronIcon size={8} className={styles.down} />
      </button>
      <span className={`${styles.prompt} mono`} aria-hidden="true">
        $
      </span>
      <input
        id={id}
        className={`${styles.commandLine} mono`}
        value={commandLine}
        placeholder="pnpm dev"
        aria-label="Command line"
        spellCheck={false}
        autoComplete="off"
        onChange={(event) => onCommandLineChange(event.target.value)}
      />
    </div>
  );
}

/** A group of rows on a quiet card, e.g. the variables table. */
export function FormCard({ children }: { children: ReactNode }) {
  return <div className={styles.card}>{children}</div>;
}

/** The class of a card's row, separated from the one above by a line. */
export const cardRow = styles.cardRow;

type ChipProps = {
  title: string;
  color: string;
  isOn: boolean;
  onChange: (isOn: boolean) => void;
  isDisabled?: boolean;
  help?: string;
};

/** A toggle drawn as a chip in a colour, e.g. a Dependency in its Service's colour. */
export function ChipToggle({ title, color, isOn, onChange, isDisabled, help }: ChipProps) {
  return (
    <button
      type="button"
      className={styles.chip}
      style={{ "--chip": color } as CSSProperties}
      aria-pressed={isOn}
      disabled={isDisabled}
      title={help}
      onClick={() => onChange(!isOn)}
    >
      <span className={styles.swatch} aria-hidden="true" />
      {title}
      {isOn && <CheckIcon size={9} />}
    </button>
  );
}

/** Chips that wrap. */
export function ChipList({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className={styles.chips} role="group" aria-label={label}>
      {children}
    </div>
  );
}

/** A setting with a switch on the right. */
export function SwitchRow(props: { title: string; detail?: string; isOn: boolean; onChange: (isOn: boolean) => void }) {
  const detailId = useId();
  return (
    <div className={styles.switchRow}>
      <div className={styles.switchText}>
        <span className={styles.switchTitle}>{props.title}</span>
        {props.detail && <FieldHint>{<span id={detailId}>{props.detail}</span>}</FieldHint>}
      </div>
      <button
        type="button"
        className={styles.switch}
        role="switch"
        aria-checked={props.isOn}
        aria-label={props.title}
        aria-describedby={props.detail ? detailId : undefined}
        onClick={() => props.onChange(!props.isOn)}
      >
        <span />
      </button>
    </div>
  );
}

/** A small spinner with a line, while something is looked for. */
export function Working({ children }: { children: ReactNode }) {
  return (
    <div className={`${styles.working} secondary`} role="status">
      <span className={styles.spinner} aria-hidden="true" />
      {children}
    </div>
  );
}

/**
 * A Key field that follows the name, as the core suggests it, until the user
 * edits it. `suggest` returns `null` when it can't suggest one.
 */
export function useFollowingKey(initial: string, isInitiallyEdited: boolean, suggest: (name: string) => Promise<string | null>) {
  const [key, setKey] = useState(initial);
  const isEdited = useRef(isInitiallyEdited);
  const latest = useRef(0);
  return {
    key,
    /** The name changed: suggest a Key for it, unless the Key was edited. */
    follow(name: string) {
      if (isEdited.current) return;
      const request = ++latest.current;
      void suggest(name).then((suggested) => {
        if (suggested !== null && request === latest.current && !isEdited.current) setKey(suggested);
      });
    },
    edit(key: string) {
      isEdited.current = true;
      setKey(key);
    },
  };
}
