import { type CSSProperties, type Dispatch, type SetStateAction, useEffect, useRef, useState } from "react";
import { commands, type Project, type ProjectFolder } from "../bindings";
import {
  canSplit,
  directoryLabel,
  draftStartsAfter,
  editedSuggestion,
  mergingDrafts,
  splittingDraft,
  type SuggestionDraft,
  suggestionDrafts,
  suggestionEdits,
} from "../editors";
import { tildePath } from "../paths";
import { portVariable, projectFolder } from "../targets";
import { closeSheet } from "../ui";
import {
  ChipList,
  ChipToggle,
  CommandField,
  Field,
  FieldHint,
  FieldRow,
  FormCard,
  FormSheet,
  SwitchRow,
  TextField,
  Working,
  cardRow,
} from "./Form";
import { ChevronIcon, DetectIcon } from "./Icons";
import { serviceColor } from "./ServicesColumn";
import styles from "./SuggestionReview.module.css";

type Props = {
  drafts: SuggestionDraft[];
  setDrafts: Dispatch<SetStateAction<SuggestionDraft[]>>;
  isScanning: boolean;
  /** The Project's folder, where working directories are chosen. */
  project: ProjectFolder;
  /** How many Services the Project already has, so new ones show the colours they'll keep. */
  existingServiceCount?: number;
  emptyMessage: string;
};

/**
 * The review step after scanning a folder: the Suggestions that are likely
 * Services (checked) and, folded away, the rest of what was found. Each row
 * can be checked, unchecked, and opened to edit it, choose what it starts
 * after, or split it; several can be merged into one. Nothing is saved here.
 */
export function SuggestionReview({ drafts, setDrafts, isScanning, project, existingServiceCount = 0, emptyMessage }: Props) {
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [isShowingOthers, setIsShowingOthers] = useState(false);
  /** The drafts picked to merge, while picking them. */
  const [merging, setMerging] = useState<number[] | null>(null);
  const checked = drafts.filter((draft) => draft.isChecked);
  const likely = drafts.filter((draft) => draft.suggestion.isRecommended);
  const others = drafts.filter((draft) => !draft.suggestion.isRecommended);
  const update = (id: number, change: Partial<SuggestionDraft>) =>
    setDrafts((drafts) => drafts.map((draft) => (draft.id === id ? { ...draft, ...change } : draft)));

  /** A checked draft's colour is the one its Service will get; unchecked ones are grey. */
  const color = (draft: SuggestionDraft) => {
    const index = checked.indexOf(draft);
    return index < 0 ? "var(--state-stopped)" : serviceColor(existingServiceCount + index);
  };

  const merge = async () => {
    if (merging === null) return;
    const parts = drafts.filter((draft) => merging.includes(draft.id));
    const merged = await commands.mergeSuggestions(parts.map(editedSuggestion));
    if (merged === null) return;
    const next = mergingDrafts(drafts, merging, merged);
    setDrafts(next);
    setMerging(null);
    setExpandedId(next.find((draft) => !drafts.includes(draft))?.id ?? null);
  };

  const split = (draft: SuggestionDraft) => {
    if (draft.mergedFrom.length === 0) setIsShowingOthers(true);
    setDrafts((drafts) => splittingDraft(drafts, draft.id));
    setExpandedId(null);
  };

  const card = (rows: SuggestionDraft[]) => (
    <FormCard>
      {rows.map((draft) => (
        <SuggestionRow
          key={draft.id}
          draft={draft}
          color={color(draft)}
          project={project}
          isExpanded={merging === null && expandedId === draft.id}
          onToggleExpanded={() => setExpandedId(expandedId === draft.id ? null : draft.id)}
          onChange={(change) => update(draft.id, change)}
          startsAfter={checked
            .filter((other) => other.id !== draft.id)
            .map((other) => ({ draft: other, color: color(other), isCycle: draftStartsAfter(drafts, other, draft) }))}
          splitTitle={
            canSplit(drafts, draft) ? (draft.mergedFrom.length > 0 ? "Undo merge" : "One Service per container") : null
          }
          onSplit={() => split(draft)}
          isPickedToMerge={merging === null ? null : merging.includes(draft.id)}
          onPickToMerge={(isPicked) =>
            setMerging((ids) => (ids === null ? ids : isPicked ? [...ids, draft.id] : ids.filter((id) => id !== draft.id)))
          }
        />
      ))}
    </FormCard>
  );

  const note =
    isScanning || drafts.length === 0
      ? null
      : merging !== null
        ? `${merging.length} to merge`
        : checked.length === 0
          ? "none selected"
          : `${checked.length} selected · edit before adding`;
  const showsOthers = isShowingOthers || likely.length === 0 || merging !== null;
  const hasContainers = others.some((draft) => (draft.suggestion.composeServices ?? []).length === 1);
  return (
    <Field title="Services" note={note}>
      {() =>
        isScanning ? (
          <Working>Looking for package.json, Compose, Python and Cargo files…</Working>
        ) : drafts.length === 0 ? (
          <FieldHint>{emptyMessage}</FieldHint>
        ) : (
          <div className={styles.groups}>
            {likely.length > 0 && card(likely)}
            {others.length > 0 && (
              <>
                <button
                  type="button"
                  className={`${styles.disclosure} secondary`}
                  aria-expanded={showsOthers}
                  disabled={likely.length === 0 || merging !== null}
                  onClick={() => setIsShowingOthers(!isShowingOthers)}
                >
                  <ChevronIcon size={9} className={showsOthers ? styles.open : undefined} />
                  {others.length} more found
                  <span className={styles.tertiary}>
                    · {hasContainers ? "one per container, build steps, other scripts" : "build steps, other scripts"}
                  </span>
                </button>
                {showsOthers && card(others)}
              </>
            )}
            {drafts.length > 1 && (
              <div className={styles.mergeBar}>
                {merging === null ? (
                  <>
                    <button type="button" className="button" onClick={() => setMerging([])}>
                      Merge…
                    </button>
                    <span className={`${styles.mergeHint} secondary`}>Run several as one Service, e.g. a Compose file</span>
                  </>
                ) : (
                  <>
                    <span className={`${styles.mergeHint} secondary`}>Pick the ones to run as one Service.</span>
                    <button type="button" className="button" onClick={() => setMerging(null)}>
                      Cancel
                    </button>
                    <button
                      type="button"
                      className="button prominent"
                      disabled={merging.length < 2}
                      onClick={() => void merge()}
                    >
                      Merge into one Service
                    </button>
                  </>
                )}
              </div>
            )}
          </div>
        )
      }
    </Field>
  );
}

type RowProps = {
  draft: SuggestionDraft;
  color: string;
  project: ProjectFolder;
  isExpanded: boolean;
  onToggleExpanded: () => void;
  onChange: (change: Partial<SuggestionDraft>) => void;
  /** The other checked drafts it can start after; a cycle when one already starts after it. */
  startsAfter: { draft: SuggestionDraft; color: string; isCycle: boolean }[];
  /** "Undo merge" or "One Service per container", when it can be split. */
  splitTitle: string | null;
  onSplit: () => void;
  /** While picking drafts to merge, whether this one is picked; else `null`. */
  isPickedToMerge: boolean | null;
  onPickToMerge: (isPicked: boolean) => void;
};

/** One Suggestion: a checkbox, its name, Key, command and Port, and when opened, fields to edit it and where it was found. */
function SuggestionRow(props: RowProps) {
  const { draft, color, project, isExpanded, onToggleExpanded, onChange, startsAfter, isPickedToMerge } = props;
  const port = /^\d+$/.test(draft.port.trim()) ? draft.port.trim() : null;
  const latestName = useRef(draft.name);
  const setName = (name: string) => {
    latestName.current = name;
    onChange({ name });
    if (!draft.isKeyEdited) {
      void commands.keyFromName(name).then((key) => {
        if (latestName.current === name) onChange({ key });
      });
    }
  };
  const chooseDirectory = async () => {
    const workingDirectory = await commands.chooseWorkingDirectory(project);
    if (workingDirectory !== null) onChange({ workingDirectory });
  };
  const after = draft.isChecked ? startsAfter.filter((other) => draft.startsAfter.includes(other.draft.id)) : [];
  const isMerging = isPickedToMerge !== null;
  return (
    <div
      className={`${styles.row} ${cardRow} ${(isMerging ? isPickedToMerge : draft.isChecked) ? "" : styles.unchecked}`}
      style={{ "--service": color } as CSSProperties}
    >
      <div className={styles.summary} title={`From ${draft.suggestion.foundIn} · ${draft.suggestion.detail}`}>
        <input
          type="checkbox"
          className={styles.checkbox}
          checked={isMerging ? isPickedToMerge : draft.isChecked}
          aria-label={isMerging ? `Merge ${draft.name}` : `Add ${draft.name}`}
          onChange={(event) =>
            isMerging ? props.onPickToMerge(event.target.checked) : onChange({ isChecked: event.target.checked })
          }
        />
        <span className={styles.swatch} aria-hidden="true" />
        <div className={styles.text} onClick={isMerging ? () => props.onPickToMerge(!isPickedToMerge) : onToggleExpanded}>
          <div className={styles.title}>
            <span className={styles.name}>{draft.name}</span>
            <span className="mono secondary">{draft.key}</span>
            {after.length > 0 && (
              <span className={styles.tertiary}>after {after.map((other) => other.draft.name).join(", ")}</span>
            )}
          </div>
          <div className={`${styles.commandLine} mono secondary`}>
            {directoryLabel(draft.workingDirectory)} $ {draft.commandLine}
          </div>
        </div>
        {port && <span className="portPill">:{port}</span>}
        {!isMerging && (
          <button
            type="button"
            className={`iconButton ${styles.expand}`}
            aria-expanded={isExpanded}
            aria-label={isExpanded ? "Close Editor" : `Edit ${draft.name}`}
            onClick={onToggleExpanded}
          >
            <ChevronIcon size={9} className={isExpanded ? styles.up : styles.down} />
          </button>
        )}
      </div>
      {isExpanded && (
        <div className={styles.editor}>
          <FieldRow>
            <Field title="Name">
              {(id) => <TextField id={id} value={draft.name} onChange={setName} placeholder="Web" autoFocus />}
            </Field>
            <Field title="Key" width={140}>
              {(id) => (
                <TextField
                  id={id}
                  value={draft.key}
                  onChange={(key) => onChange({ key, isKeyEdited: true })}
                  placeholder="web"
                  isMonospaced
                />
              )}
            </Field>
            <Field title="Port" width={80}>
              {(id) => (
                <TextField id={id} value={draft.port} onChange={(value) => onChange({ port: value })} placeholder="—" isMonospaced />
              )}
            </Field>
          </FieldRow>
          <Field title="Command">
            {(id) => (
              <CommandField
                id={id}
                workingDirectory={draft.workingDirectory}
                commandLine={draft.commandLine}
                onCommandLineChange={(commandLine) => onChange({ commandLine })}
                onChooseDirectory={() => void chooseDirectory()}
              />
            )}
          </Field>
          {draft.port.trim() !== "" && (
            <SwitchRow
              title="Port can change"
              detail={
                draft.portCanChange
                  ? `If the port is in use, a run may start on the next free one (passed as ${portVariable(project.target)}).`
                  : "The port is fixed by the command or a Compose file, so it can’t move."
              }
              isOn={draft.portCanChange}
              onChange={(portCanChange) => onChange({ portCanChange })}
            />
          )}
          {draft.isChecked && startsAfter.length > 0 && (
            <Field
              title="Starts after"
              note="waits until they’re running"
              hint={
                startsAfter.some((other) => other.isCycle)
                  ? "Services that already start after this one can’t be chosen: that would be a cycle."
                  : null
              }
            >
              {() => (
                <ChipList label={`${draft.name} starts after`}>
                  {startsAfter.map((other) => {
                    const isOn = draft.startsAfter.includes(other.draft.id);
                    return (
                      <ChipToggle
                        key={other.draft.id}
                        title={other.draft.name}
                        color={other.color}
                        isOn={isOn}
                        isDisabled={other.isCycle && !isOn}
                        help={other.isCycle ? `${other.draft.name} already starts after ${draft.name}.` : other.draft.key}
                        onChange={(isOn) =>
                          onChange({
                            startsAfter: isOn
                              ? [...draft.startsAfter, other.draft.id]
                              : draft.startsAfter.filter((id) => id !== other.draft.id),
                          })
                        }
                      />
                    );
                  })}
                </ChipList>
              )}
            </Field>
          )}
          <div className={styles.footer}>
            <p className={`${styles.foundIn} secondary`}>
              Found in <span className="mono">{draft.suggestion.foundIn}</span> ·{" "}
              <span className="mono">{draft.suggestion.detail}</span>
            </p>
            {props.splitTitle && (
              <button type="button" className="button" onClick={props.onSplit}>
                {props.splitTitle}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** Scans an existing Project's folder again and adds the Services the user picks from what it finds. */
export function DetectServicesForm({ project, onAdded }: { project: Project; onAdded: (serviceId: string) => void }) {
  const [drafts, setDrafts] = useState<SuggestionDraft[]>([]);
  const [isScanning, setIsScanning] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    let isCurrent = true;
    void commands.scanProject(project.id).then((result) => {
      if (!isCurrent) return;
      if (result.status === "ok") setDrafts(suggestionDrafts(result.data));
      else setError(result.error.message);
      setIsScanning(false);
    });
    return () => {
      isCurrent = false;
    };
  }, [project.id]);

  const count = drafts.filter((draft) => draft.isChecked).length;
  const save = async () => {
    setIsSaving(true);
    const result = await commands.addServices(project.id, suggestionEdits(drafts));
    setIsSaving(false);
    if (result.status === "error") return setError(result.error.message);
    closeSheet();
    if (result.data[0]) onAdded(result.data[0].id);
  };
  return (
    <FormSheet
      icon={<DetectIcon size={15} />}
      title="Detect Services"
      subtitle={
        <>
          in <strong>{project.name}</strong> · <span className="mono">{tildePath(project.path)}</span>
        </>
      }
      hint={count === 0 ? null : "↩ to add"}
      error={error}
      primaryTitle={count === 0 ? "Add Services" : `Add ${count} Service${count === 1 ? "" : "s"}`}
      isPrimaryDisabled={count === 0 || isSaving}
      width={640}
      onCancel={closeSheet}
      onSubmit={() => void save()}
    >
      <SuggestionReview
        drafts={drafts}
        setDrafts={setDrafts}
        isScanning={isScanning}
        project={projectFolder(project)}
        existingServiceCount={project.services.length}
        emptyMessage="Nothing new found. Services this project already runs aren’t suggested again."
      />
    </FormSheet>
  );
}
