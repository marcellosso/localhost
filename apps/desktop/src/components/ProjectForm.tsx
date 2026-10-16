import { useEffect, useRef, useState } from "react";
import { commands, type ExecutionTarget, type Project, type ProjectFolder } from "../bindings";
import { type SuggestionDraft, suggestionDrafts, suggestionEdits } from "../editors";
import { folderName, isSameTarget, projectFolder, targetId, thisMachine } from "../targets";
import { closeSheet } from "../ui";
import { Field, FieldRow, FolderField, FormSheet, TargetField, TextField, useFollowingKey } from "./Form";
import { FolderIcon } from "./Icons";
import { SuggestionReview } from "./SuggestionReview";

type Props = {
  /** The Project to edit; a new one when absent. */
  editing?: Project;
  onAdded?: (project: Project) => void;
};

/**
 * Adds a Project (folder, name, Key derived from the name) or edits one.
 * When adding, the folder is scanned as soon as it is chosen and the
 * Suggestions are reviewed in the same sheet, then saved with the Project.
 *
 * On Windows it also says where the Project runs: on Windows itself (the
 * default for a new Project) or in a WSL distro. A folder picked inside a
 * distro runs there; one on a Windows drive, on Windows itself, or in the
 * chosen distro through its mount.
 */
export function ProjectForm({ editing, onAdded }: Props) {
  const [folder, setFolder] = useState<ProjectFolder | null>(editing ? projectFolder(editing) : null);
  const [target, setTarget] = useState<ExecutionTarget>(editing ? projectFolder(editing).target : thisMachine);
  const [distros, setDistros] = useState<string[]>([]);
  const [name, setName] = useState(editing?.name ?? "");
  const key = useFollowingKey(editing?.key ?? "", editing !== undefined, async (name) => {
    const result = await commands.suggestedProjectKey(name, editing?.id ?? null);
    return result.status === "ok" ? result.data : null;
  });
  const [drafts, setDrafts] = useState<SuggestionDraft[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  /** The folder the shown Suggestions are for, so a late scan of an earlier folder is ignored. */
  const scannedFolder = useRef("");

  useEffect(() => {
    void commands.wslDistros().then(setDistros);
  }, []);

  const changeName = (value: string) => {
    setName(value);
    key.follow(value);
  };

  /** Another target: a folder chosen on the previous one no longer applies. */
  const changeTarget = (next: ExecutionTarget) => {
    setTarget(next);
    if (folder && !isSameTarget(folder.target, next)) {
      setFolder(editing ? { target: next, path: "" } : null);
      scannedFolder.current = "";
      setDrafts([]);
      setIsScanning(false);
    }
  };

  const chooseFolder = async () => {
    const start = folder?.path && isSameTarget(folder.target, target) ? folder.path : null;
    const chosen = await commands.chooseFolder(target, start);
    if (chosen === null) return;
    setFolder(chosen);
    setTarget(chosen.target);
    if (!name) changeName(folderName(chosen.path));
    if (!editing) void scan(chosen);
  };

  const scan = async (chosen: ProjectFolder) => {
    const scanned = `${targetId(chosen.target)}:${chosen.path}`;
    scannedFolder.current = scanned;
    setDrafts([]);
    setIsScanning(true);
    const result = await commands.scanFolder(chosen);
    if (scannedFolder.current !== scanned) return;
    if (result.status === "ok") setDrafts(suggestionDrafts(result.data));
    else setError(result.error.message);
    setIsScanning(false);
  };

  const save = async () => {
    if (!folder) return;
    setIsSaving(true);
    setError(null);
    if (editing) {
      const result = await commands.editProject(editing.id, name, key.key, folder);
      setIsSaving(false);
      if (result.status === "error") return setError(result.error.message);
      closeSheet();
    } else {
      const result = await commands.addProject(name, key.key, folder, suggestionEdits(drafts));
      setIsSaving(false);
      if (result.status === "error") return setError(result.error.message);
      closeSheet();
      onAdded?.(result.data);
    }
  };

  const path = folder?.path ?? "";
  const moves = editing !== undefined && !isSameTarget(projectFolder(editing).target, target);
  const count = drafts.filter((draft) => draft.isChecked).length;
  const hint = editing ? "↩ to save" : count === 0 ? "↩ to add" : `↩ to add with ${count} service${count === 1 ? "" : "s"}`;
  return (
    <FormSheet
      icon={<FolderIcon size={16} />}
      title={editing ? `Edit ${editing.name}` : "New Project"}
      subtitle="A folder whose services localhost runs."
      hint={hint}
      error={error}
      primaryTitle={editing ? "Save" : "Add Project"}
      isPrimaryDisabled={!path || !name.trim() || !key.key.trim() || isScanning || isSaving}
      width={!editing && path ? 640 : 500}
      onCancel={closeSheet}
      onSubmit={() => void save()}
    >
      {(distros.length > 0 || target.kind === "wsl") && (
        <Field title="Runs in" hint={moves ? "Its services are stopped first, and their logs where it ran are removed." : null}>
          {(id) => <TargetField id={id} target={target} distros={distros} onChange={changeTarget} />}
        </Field>
      )}
      <Field title="Folder">
        {(id) => <FolderField id={id} path={path} target={folder?.target} onChoose={() => void chooseFolder()} />}
      </Field>
      <FieldRow>
        <Field title="Name">
          {(id) => <TextField id={id} value={name} onChange={changeName} placeholder="My App" autoFocus={!!editing} />}
        </Field>
        <Field title="Key" note="in logs & CLI" width={160}>
          {(id) => <TextField id={id} value={key.key} onChange={key.edit} placeholder="my-app" isMonospaced />}
        </Field>
      </FieldRow>
      {!editing && path && (
        <SuggestionReview
          drafts={drafts}
          setDrafts={setDrafts}
          isScanning={isScanning}
          project={folder ?? { target, path }}
          emptyMessage="Nothing recognised in this folder. Add the project, then add its services yourself."
        />
      )}
    </FormSheet>
  );
}
