import { type CSSProperties, useEffect, useId, useMemo, useRef, useState } from "react";
import type { Project } from "../bindings";
import type { CoreView } from "../core";
import { focusColumn } from "../keyboard";
import { emptyQueryResults, type Navigator, type PaletteIcon, type PaletteItem, paletteItems, rankItems } from "../palette";
import type { ServiceSelection } from "../selection";
import { setPaletteOpen } from "../ui";
import {
  ArrowUpRightIcon,
  ChevronIcon,
  FolderIcon,
  GlobeIcon,
  PlayIcon,
  PlusIcon,
  SearchIcon,
  StopIcon,
  TerminalIcon,
} from "./Icons";
import { serviceColor } from "./ServicesColumn";
import { StateGlyph } from "./StateGlyph";
import styles from "./CommandPalette.module.css";

type Props = {
  view: CoreView;
  selected: Project | undefined;
  selection: ServiceSelection;
  navigator: Navigator;
};

/**
 * The command palette (Ctrl+K): a search field over every Project, Service
 * and action. ↑/↓ (or Ctrl+P/Ctrl+N) move, ↩ runs, Escape or a click outside
 * closes, and focus goes back where it was.
 */
export function CommandPalette({ view, selected, selection, navigator }: Props) {
  const [query, setQuery] = useState("");
  const items = useMemo(() => paletteItems(view, selected, selection, navigator), [view, selected, selection, navigator]);
  const [results, setResults] = useState(() => emptyQueryResults(items));
  const [selectedId, setSelectedId] = useState<string | null>(() => results[0]?.id ?? null);
  const listId = useId();
  const optionId = (item: PaletteItem) => `${listId}-${item.id}`;
  const previousFocus = useRef(document.activeElement as HTMLElement | null);

  // The latest query's results only, however the requests return.
  const request = useRef(0);
  useEffect(() => {
    const current = ++request.current;
    void rankItems(query, items).then((ranked) => {
      if (current !== request.current) return;
      setResults(ranked);
      setSelectedId((id) => (ranked.some((item) => item.id === id) ? id : (ranked[0]?.id ?? null)));
    });
  }, [query, items]);

  // A new query starts at its best match.
  useEffect(() => setSelectedId(null), [query]);
  const selectedIndex = Math.max(
    results.findIndex((item) => item.id === selectedId),
    0,
  );
  const current = results[selectedIndex];

  // Only a keyboard move scrolls: scrolling under a resting pointer would
  // hover another row, select it, and scroll again.
  const scrollsToSelection = useRef(false);
  useEffect(() => {
    if (!scrollsToSelection.current || !current) return;
    scrollsToSelection.current = false;
    document.getElementById(optionId(current))?.scrollIntoView?.({ block: "nearest" });
  }, [current?.id]);

  const close = () => {
    setPaletteOpen(false);
    // After the window shows what the item chose, e.g. another Project.
    requestAnimationFrame(() => {
      const previous = previousFocus.current;
      if (previous?.isConnected && previous !== document.body) previous.focus();
      else if (!focusColumn("list")) focusColumn("sidebar");
    });
  };
  const run = (item: PaletteItem | undefined) => {
    if (!item) return;
    close();
    item.run();
  };
  const move = (offset: number) => {
    if (results.length === 0) return;
    scrollsToSelection.current = true;
    setSelectedId(results[Math.min(Math.max(selectedIndex + offset, 0), results.length - 1)].id);
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const isControl = event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
    if (event.key === "ArrowDown" || (isControl && event.code === "KeyN")) move(1);
    else if (event.key === "ArrowUp" || (isControl && event.code === "KeyP")) move(-1);
    else if (event.key === "Enter") run(current);
    else if (event.key === "Escape") close();
    else if (event.key === "Tab") {
      // The field is the palette's only stop.
    } else return;
    event.preventDefault();
    event.stopPropagation();
  };

  return (
    <div className={styles.backdrop} onMouseDown={(event) => event.target === event.currentTarget && close()}>
      <div className={styles.palette} role="dialog" aria-modal="true" aria-label="Command Palette">
        <div className={styles.field}>
          <SearchIcon size={14} className="secondary" />
          <input
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-activedescendant={current ? optionId(current) : undefined}
            aria-label="Jump to a project or service, or run an action"
            placeholder="Jump to a project or service, or run an action…"
            value={query}
            spellCheck={false}
            autoComplete="off"
            autoFocus
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={onKeyDown}
          />
          <span className="keyHint">esc</span>
        </div>
        {(results.length > 0 || query) && (
          <ul id={listId} className={styles.list} role="listbox" aria-label="Results">
            {results.length === 0 && (
              <li className={`${styles.empty} secondary`} role="presentation">
                No matches for “{query}”
              </li>
            )}
            {results.map((item) => (
              <li
                key={item.id}
                id={optionId(item)}
                className={styles.row}
                role="option"
                aria-selected={item === current}
                onMouseMove={(event) => {
                  // A real move of the pointer, not the list scrolling under it.
                  if (item !== current && (event.movementX !== 0 || event.movementY !== 0)) setSelectedId(item.id);
                }}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => run(item)}
              >
                <span className={styles.icon}>
                  <ItemIcon icon={item.icon} />
                </span>
                <span className={styles.text}>
                  <span className={styles.title}>{item.title}</span>
                  {item.subtitle && <span className={`${styles.subtitle} mono secondary`}>{item.subtitle}</span>}
                </span>
                {item.shortcut && <span className="keyHint">{item.shortcut}</span>}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function ItemIcon({ icon }: { icon: PaletteIcon }) {
  if (icon.kind === "service") {
    return (
      <span className={styles.service} style={{ "--service": serviceColor(icon.index) } as CSSProperties}>
        <span className={styles.rail} aria-hidden="true" />
        <StateGlyph state={icon.state} />
      </span>
    );
  }
  switch (icon.name) {
    case "folder":
      return <FolderIcon size={13} />;
    case "ports":
      return <GlobeIcon size={13} />;
    case "new":
      return <PlusIcon size={12} />;
    case "play":
    case "restart":
      return <PlayIcon size={11} />;
    case "stop":
      return <StopIcon size={11} />;
    case "browser":
      return <ArrowUpRightIcon size={11} />;
    case "terminal":
      return <TerminalIcon size={13} />;
    case "editor":
      return <ChevronIcon size={11} />;
    case "service":
      return <TerminalIcon size={13} />;
  }
}
