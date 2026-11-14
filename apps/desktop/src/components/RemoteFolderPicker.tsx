import { useEffect, useId, useRef, useState } from "react";
import { commands, type DirectoryEntry, type DirectoryListing } from "../bindings";
import { childPath, isProjectFile, pathCrumbs, type RemotePick, useRemotePick } from "../remoteFolders";
import { FormSheet, SwitchRow, Working } from "./Form";
import { ChevronIcon, FolderIcon, ImportIcon, LinesIcon } from "./Icons";
import styles from "./RemoteFolderPicker.module.css";

/** The remote folder picker on screen, if one was asked for (`chooseOnMachine`). */
export function RemoteFolderPicker() {
  const pick = useRemotePick();
  // A new request starts afresh.
  return pick ? <Picker key={`${pick.machine.id}:${pick.start}:${pick.isFile}`} pick={pick} /> : null;
}

/**
 * A folder of another Machine, browsed through its localhost: the path
 * (typed, or a breadcrumb), its folders (and Project files when picking a
 * file), Show hidden, then Choose. What can't be listed says why, in the
 * Machine's words.
 */
function Picker({ pick }: { pick: RemotePick }) {
  const { machine, isFile } = pick;
  const [listing, setListing] = useState<DirectoryListing | null>(null);
  const [typed, setTyped] = useState(pick.start ?? "");
  const [showsHidden, setShowsHidden] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const latest = useRef(0);
  const listId = useId();

  const open = async (path: string | null, hidden = showsHidden) => {
    const request = ++latest.current;
    setIsLoading(true);
    const result = await commands.machineListDirectory(machine.id, path, hidden, isFile);
    if (request !== latest.current) return;
    setIsLoading(false);
    if (result.status === "error") {
      // Where it was asked to start may be gone: home instead, once.
      if (path !== null && listing === null && path === pick.start) return void open(null, hidden);
      setError(result.error.message);
      return;
    }
    setError(null);
    setListing(result.data);
    setTyped(result.data.path);
    setSelected(null);
  };

  useEffect(() => {
    void open(pick.start);
    // Only as it opens.
  }, []);

  const entries: DirectoryEntry[] = (listing?.entries ?? []).filter(
    (entry) => entry.isDirectory || isProjectFile(entry.name),
  );
  const pathOf = (entry: DirectoryEntry) => childPath(listing?.path ?? "", entry.name);
  const activate = (entry: DirectoryEntry) => {
    if (entry.isDirectory) void open(pathOf(entry));
    else pick.resolve(pathOf(entry));
  };
  const selectedEntry = entries.find((entry) => pathOf(entry) === selected);

  const choose = () => {
    if (!listing) return;
    if (isFile) {
      if (selectedEntry && !selectedEntry.isDirectory) pick.resolve(pathOf(selectedEntry));
      else if (selectedEntry) void open(pathOf(selectedEntry));
    } else {
      pick.resolve(selectedEntry?.isDirectory ? pathOf(selectedEntry) : listing.path);
    }
  };

  const onListKey = (event: React.KeyboardEvent) => {
    const index = entries.findIndex((entry) => entry === selectedEntry);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = event.key === "ArrowDown" ? Math.min(index + 1, entries.length - 1) : Math.max(index - 1, 0);
      if (entries[next]) setSelected(pathOf(entries[next]));
    } else if (event.key === "Enter" && selectedEntry) {
      event.preventDefault();
      event.stopPropagation();
      activate(selectedEntry);
    } else if (event.key === "Backspace" && listing?.parent) {
      event.preventDefault();
      void open(listing.parent);
    }
  };

  const crumbs = listing ? pathCrumbs(listing.path) : [];
  const title = isFile ? `Choose a Project File on ${machine.name}` : `Choose a Folder on ${machine.name}`;
  const canChoose = listing !== null && !isLoading && (!isFile || (selectedEntry !== undefined && !selectedEntry.isDirectory));
  return (
    <FormSheet
      icon={isFile ? <ImportIcon size={16} /> : <FolderIcon size={16} />}
      title={title}
      subtitle={isFile ? "A localhost Project file (YAML) on that machine." : "Folders on that machine, as its localhost sees them."}
      hint={isFile ? "↩ opens a folder or picks a file" : selectedEntry ? "↩ opens the folder" : null}
      error={error}
      primaryTitle={isFile ? "Choose" : selectedEntry?.isDirectory ? `Choose “${selectedEntry.name}”` : "Choose"}
      isPrimaryDisabled={!canChoose}
      width={560}
      onCancel={() => pick.resolve(null)}
      onSubmit={choose}
    >
      <div className={styles.pathRow}>
        <input
          className={`${styles.path} mono`}
          value={typed}
          aria-label={`Path on ${machine.name}`}
          placeholder="/home/me/projects"
          spellCheck={false}
          autoComplete="off"
          onChange={(event) => setTyped(event.target.value)}
          onKeyDown={(event) => {
            // ↩ here goes to the typed path rather than choosing.
            if (event.key === "Enter") {
              event.preventDefault();
              void open(typed.trim() || null);
            }
          }}
        />
        <button type="button" className="button" onClick={() => void open(typed.trim() || null)}>
          Go
        </button>
      </div>
      <nav className={styles.crumbs} aria-label="Path">
        <button
          type="button"
          className="iconButton"
          aria-label="Enclosing folder"
          title="Enclosing folder (⌫)"
          disabled={!listing?.parent}
          onClick={() => listing?.parent && void open(listing.parent)}
        >
          <ChevronIcon size={9} className={styles.up} />
        </button>
        {crumbs.map((crumb, index) => (
          <span key={crumb.path} className={styles.crumb}>
            {index > 1 && <span className="secondary" aria-hidden="true">/</span>}
            <button
              type="button"
              className={`${styles.crumbButton} mono`}
              aria-current={index === crumbs.length - 1 ? "location" : undefined}
              onClick={() => void open(crumb.path)}
            >
              {crumb.name}
            </button>
          </span>
        ))}
        {listing && listing.path !== listing.home && (
          <button type="button" className={`${styles.home} button compact`} onClick={() => void open(listing.home)}>
            Home
          </button>
        )}
      </nav>
      <div className={styles.list}>
        {isLoading && !listing ? (
          <Working>Reading {machine.name}…</Working>
        ) : entries.length === 0 ? (
          <p className={`${styles.empty} secondary`}>
            {error ? "Nothing to show." : isFile ? "No folders or Project files here." : "No folders here."}
          </p>
        ) : (
          <ul
            role="listbox"
            aria-label={listing ? `Contents of ${listing.path}` : "Contents"}
            aria-activedescendant={selectedEntry ? `${listId}-${selectedEntry.name}` : undefined}
            tabIndex={0}
            data-autofocus
            onKeyDown={onListKey}
          >
            {entries.map((entry) => (
              <li
                key={entry.name}
                id={`${listId}-${entry.name}`}
                className={`${styles.entry} selectable`}
                role="option"
                aria-selected={entry === selectedEntry}
                aria-label={`${entry.name}, ${entry.isDirectory ? "folder" : "Project file"}`}
                onClick={() => setSelected(pathOf(entry))}
                onDoubleClick={() => activate(entry)}
              >
                {entry.isDirectory ? <FolderIcon size={13} /> : <LinesIcon size={13} />}
                <span className={`${styles.name} mono`}>{entry.name}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
      <SwitchRow
        title="Show hidden"
        detail="Folders whose names start with a dot."
        isOn={showsHidden}
        onChange={(isOn) => {
          setShowsHidden(isOn);
          void open(listing?.path ?? pick.start, isOn);
        }}
      />
    </FormSheet>
  );
}
