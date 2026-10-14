import { type CSSProperties, useState } from "react";
import { commands, type ImportChoice, type ImportedService, type ImportPreview, type Project } from "../bindings";
import { directoryLabel } from "../editors";
import { closeSheet } from "../ui";
import { Field, FieldHint, FolderField, FormCard, FormSheet, cardRow } from "./Form";
import { ImportIcon, LockIcon } from "./Icons";
import { serviceColor } from "./ServicesColumn";
import styles from "./ImportProjectForm.module.css";

type Props = {
  file: string;
  preview: ImportPreview;
  onImported: (project: Project) => void;
};

/**
 * The review step of Import Project…: where the Project's folder is on this
 * computer, what to do if its Key is taken, and what the file brings
 * (Services, environment, and the Secrets that still need values). Nothing
 * is saved until Import or Replace.
 */
export function ImportProjectForm({ file, preview, onImported }: Props) {
  const [path, setPath] = useState(preview.path);
  const [folderExists, setFolderExists] = useState(preview.folderExists);
  const [choice, setChoice] = useState<ImportChoice>("keepBoth");
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const { clash } = preview;
  const isReplacing = clash !== null && choice === "replace";

  const chooseFolder = async () => {
    const folder = await commands.chooseFolder(folderExists ? path : null);
    if (folder === null) return;
    setPath(folder);
    setFolderExists(true);
  };

  const save = async () => {
    setIsSaving(true);
    setError(null);
    const result = await commands.importProject(file, path, clash ? choice : null);
    setIsSaving(false);
    if (result.status === "error") return setError(result.error.message);
    closeSheet();
    onImported(result.data);
  };

  const replacementHint = isReplacing && clash.removed.length > 0 ? `Removes ${clash.removed.join(", ")}, which the file doesn’t have.` : null;
  return (
    <FormSheet
      icon={<ImportIcon size={16} />}
      title={`Import ${preview.name}`}
      subtitle={
        <>
          from <span className="mono">{preview.fileName}</span>
        </>
      }
      hint={!folderExists ? null : isReplacing ? `Replaces “${clash.projectName}”` : "↩ to import"}
      error={error}
      primaryTitle={isReplacing ? "Replace" : "Import"}
      isPrimaryDisabled={!folderExists || isSaving}
      width={600}
      onCancel={closeSheet}
      onSubmit={() => void save()}
    >
      <Field title="Folder" hint={folderExists ? null : "This folder isn’t on this computer. Choose where the project is."}>
        {(id) => <FolderField id={id} path={path} onChoose={() => void chooseFolder()} />}
      </Field>
      {clash && (
        <Field title="Key" note={`“${preview.key}” is already used by ${clash.projectName}`} hint={replacementHint}>
          {() => (
            <div className={styles.choices} role="radiogroup" aria-label={`Key “${preview.key}” is already used by ${clash.projectName}`}>
              <label className={styles.choice}>
                <input type="radio" name="keyClash" checked={choice === "keepBoth"} onChange={() => setChoice("keepBoth")} />
                Keep both: import as <span className="mono">{clash.freeKey}</span>
              </label>
              <label className={styles.choice}>
                <input type="radio" name="keyClash" checked={choice === "replace"} onChange={() => setChoice("replace")} />
                Replace “{clash.projectName}”: keep its logs, use this file’s configuration
              </label>
            </div>
          )}
        </Field>
      )}
      <Field title="Services" note={preview.services.length > 0 ? String(preview.services.length) : null}>
        {() =>
          preview.services.length === 0 ? (
            <FieldHint>The file has no services.</FieldHint>
          ) : (
            <FormCard>
              {preview.services.map((service, index) => (
                <ImportedServiceRow key={service.key} service={service} color={serviceColor(index)} />
              ))}
            </FormCard>
          )
        }
      </Field>
      {preview.environment && (
        <Field title="Environment">{() => <FieldHint>{preview.environment}</FieldHint>}</Field>
      )}
      {preview.secrets.length > 0 && (
        <Field
          title="Secrets to set"
          note={String(preview.secrets.length)}
          hint={
            "Secret values aren’t in the file. Set them after importing" +
            (isReplacing ? "; values already stored under the same name for the same service or profile are kept." : ".")
          }
        >
          {() => (
            <FormCard>
              {preview.secrets.map((secret, index) => (
                <div key={index} className={`${styles.secret} ${cardRow}`}>
                  <LockIcon size={10} className="secondary" />
                  <span className="mono">{secret.name}</span>
                  <span className={`${styles.owner} secondary`}>{secret.owner}</span>
                </div>
              ))}
            </FormCard>
          )}
        </Field>
      )}
    </FormSheet>
  );
}

/** One Service of the file: its colour, name, Key, `./dir $ command`, Port and what it depends on. */
function ImportedServiceRow({ service, color }: { service: ImportedService; color: string }) {
  return (
    <div className={`${styles.service} ${cardRow}`} style={{ "--service": color } as CSSProperties}>
      <span className={styles.swatch} aria-hidden="true" />
      <div className={styles.text}>
        <div className={styles.title}>
          <span className={styles.name}>{service.name}</span>
          <span className="mono secondary">{service.key}</span>
          {service.isExcludedFromStartAll && <span className={styles.tertiary}>not in Start All</span>}
        </div>
        <div className={`${styles.commandLine} mono secondary`}>
          {directoryLabel(service.workingDirectory)} $ {service.commandLine}
        </div>
        {service.dependencies.length > 0 && (
          <div className="secondary">after {service.dependencies.join(", ")}</div>
        )}
      </div>
      {service.port !== null && <span className="portPill">:{service.port}</span>}
    </div>
  );
}
