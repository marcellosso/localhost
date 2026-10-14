import type { Dispatch, SetStateAction } from "react";
import { commands, type VariableLayer } from "../bindings";
import { report } from "../actions";
import { newVariableDraft, type VariableDraft } from "../editors";
import { FormCard } from "./Form";
import { EyeIcon, EyeSlashIcon, LockIcon, LockOpenIcon, MinusCircleIcon, PlusIcon } from "./Icons";
import styles from "./VariableEditor.module.css";

type Props = {
  variables: VariableDraft[];
  setVariables: Dispatch<SetStateAction<VariableDraft[]>>;
  /** Where stored Secrets are read from; `null` while the layer is new. */
  layer: VariableLayer | null;
  /** Names the table for screen readers, e.g. "Project variables". */
  label: string;
};

/**
 * Edits one layer's variables. Secrets are masked; the eye reveals one, the
 * lock turns a variable into a Secret or back. A stored Secret's value is only
 * read from the system's secret store when revealed or made plain.
 */
export function VariableEditor({ variables, setVariables, layer, label }: Props) {
  const update = (id: number, change: Partial<VariableDraft>) =>
    setVariables((variables) => variables.map((draft) => (draft.id === id ? { ...draft, ...change } : draft)));

  /** The draft with its stored value loaded, if it had one to load. */
  const withStoredValue = async (draft: VariableDraft): Promise<VariableDraft> => {
    if (draft.isValueKnown || !layer || draft.storedSecretName === null) return draft;
    const result = await commands.secretValue(draft.storedSecretName, layer);
    if (result.status === "error") {
      report(result);
      return draft;
    }
    return { ...draft, value: result.data ?? "", isValueKnown: true };
  };

  const toggleReveal = async (draft: VariableDraft) => {
    const loaded = draft.isRevealed ? draft : await withStoredValue(draft);
    update(draft.id, { value: loaded.value, isValueKnown: loaded.isValueKnown, isRevealed: !draft.isRevealed });
  };

  /** Making a stored Secret plain needs its value first. */
  const toggleSecret = async (draft: VariableDraft) => {
    const loaded = draft.isSecret ? await withStoredValue(draft) : draft;
    update(draft.id, { value: loaded.value, isValueKnown: loaded.isValueKnown, isSecret: !draft.isSecret, isRevealed: false });
  };

  return (
    <FormCard>
      <div role="group" aria-label={label}>
        {variables.map((draft) => {
          const name = draft.name || "variable";
          return (
            <div key={draft.id} className={styles.row}>
              <input
                className={`${styles.name} mono`}
                value={draft.name}
                placeholder="NAME"
                aria-label="Name"
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => update(draft.id, { name: event.target.value })}
              />
              <input
                className={`${styles.value} mono`}
                type={draft.isSecret && !draft.isRevealed ? "password" : "text"}
                value={draft.value}
                placeholder={draft.isValueKnown ? "value" : "Saved — type to replace"}
                aria-label={draft.name ? `Value of ${draft.name}` : "Value"}
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => update(draft.id, { value: event.target.value, isValueKnown: true })}
              />
              {draft.isSecret && (
                <button
                  type="button"
                  className="iconButton"
                  aria-label={draft.isRevealed ? `Hide the value of ${name}` : `Reveal the value of ${name}`}
                  title={draft.isRevealed ? "Hide the value" : "Reveal the value"}
                  onClick={() => void toggleReveal(draft)}
                >
                  {draft.isRevealed ? <EyeSlashIcon size={13} /> : <EyeIcon size={13} />}
                </button>
              )}
              <button
                type="button"
                className={`iconButton ${draft.isSecret ? styles.secret : ""}`}
                aria-label={draft.isSecret ? `Make ${name} a plain variable` : `Make ${name} a secret`}
                aria-pressed={draft.isSecret}
                title={
                  draft.isSecret
                    ? "Secret: stored in your system’s secret store, never in the project file. Click to make it a plain variable."
                    : "Make it a secret"
                }
                onClick={() => void toggleSecret(draft)}
              >
                {draft.isSecret ? <LockIcon size={13} /> : <LockOpenIcon size={13} />}
              </button>
              <button
                type="button"
                className="iconButton"
                aria-label={`Remove ${name}`}
                title="Remove"
                onClick={() => setVariables((variables) => variables.filter((other) => other.id !== draft.id))}
              >
                <MinusCircleIcon size={13} />
              </button>
            </div>
          );
        })}
      </div>
      <button type="button" className={`${styles.add} secondary`} onClick={() => setVariables((variables) => [...variables, newVariableDraft()])}>
        <PlusIcon size={10} />
        Add variable
      </button>
    </FormCard>
  );
}
