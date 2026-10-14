import { useRef, useState } from "react";
import { commands, type Project } from "../bindings";
import { type SuggestionDraft, suggestionDrafts, suggestionEdits } from "../editors";
import { closeSheet } from "../ui";
import { Field, FieldRow, FolderField, FormSheet, TextField, useFollowingKey } from "./Form";
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
 */
export function ProjectForm({ editing, onAdded }: Props) {
  const [path, setPath] = useState(editing?.path ?? "");
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
  const scannedPath = useRef("");

  const changeName = (value: string) => {
    setName(value);
    key.follow(value);
  };

  const chooseFolder = async () => {
    const folder = await commands.chooseFolder(path || null);
    if (folder === null) return;
    setPath(folder);
    if (!name) changeName(folder.split("/").filter(Boolean).pop() ?? "");
    if (!editing) void scan(folder);
  };

  const scan = async (folder: string) => {
    scannedPath.current = folder;
    setDrafts([]);
    setIsScanning(true);
    const result = await commands.scanFolder(folder);
    if (scannedPath.current !== folder) return;
    if (result.status === "ok") setDrafts(suggestionDrafts(result.data));
    else setError(result.error.message);
    setIsScanning(false);
  };

  const save = async () => {
    setIsSaving(true);
    setError(null);
    if (editing) {
      const result = await commands.editProject(editing.id, name, key.key, path);
      setIsSaving(false);
      if (result.status === "error") return setError(result.error.message);
      closeSheet();
    } else {
      const result = await commands.addProject(name, key.key, path, suggestionEdits(drafts));
      setIsSaving(false);
      if (result.status === "error") return setError(result.error.message);
      closeSheet();
      onAdded?.(result.data);
    }
  };

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
      <Field title="Folder">{(id) => <FolderField id={id} path={path} onChoose={() => void chooseFolder()} />}</Field>
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
          projectPath={path}
          emptyMessage="Nothing recognised in this folder. Add the project, then add its services yourself."
        />
      )}
    </FormSheet>
  );
}
