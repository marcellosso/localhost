import {
  commands,
  type Id,
  type Project,
  type Service,
  type ServiceSuggestion,
  type SuggestionEdit,
  type VariableEdit,
  type Variables,
} from "./bindings";
import { report } from "./actions";
import { showAlert, showSheet } from "./ui";

// What the editors work on before saving: variable rows and reviewed
// Suggestions, and the actions around them (import, export, favourite, the
// active Profile). Parsing and validation happen in the core, on save.

let nextDraftId = 0;
const draftId = () => ++nextDraftId;

/** One variable row being edited. A stored Secret has no value here until it is revealed or typed again. */
export type VariableDraft = {
  id: number;
  name: string;
  value: string;
  isSecret: boolean;
  /** The name a Secret loaded from the configuration is stored under. */
  storedSecretName: string | null;
  /** Whether `value` holds the value (typed, or revealed). */
  isValueKnown: boolean;
  isRevealed: boolean;
};

export function newVariableDraft(): VariableDraft {
  return { id: draftId(), name: "", value: "", isSecret: false, storedSecretName: null, isValueKnown: true, isRevealed: false };
}

/** Plain variables first, then Secrets, each sorted by name. */
export function variableDrafts(variables: Variables | undefined): VariableDraft[] {
  if (!variables) return [];
  const plain = Object.entries(variables.values)
    .toSorted(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([name, value]) => ({ ...newVariableDraft(), name, value: value ?? "" }));
  const secrets = variables.secretNames.map((name) => ({
    ...newVariableDraft(),
    name,
    isSecret: true,
    storedSecretName: name,
    isValueKnown: false,
  }));
  return [...plain, ...secrets];
}

/** The service's variables, as its layer. */
export function serviceVariables(service: Service): Variables {
  return { values: service.environment, secretNames: service.secretNames ?? [] };
}

/** What to save; empty rows are left out. */
export function variableEdits(drafts: VariableDraft[]): VariableEdit[] {
  return drafts
    .filter((draft) => draft.name !== "" || draft.value !== "")
    .map((draft): VariableEdit => {
      if (!draft.isSecret) return { name: draft.name, value: { kind: "plain", value: draft.value } };
      if (draft.storedSecretName !== null && !draft.isValueKnown) {
        return { name: draft.name, value: { kind: "storedSecret", previousName: draft.storedSecretName } };
      }
      return { name: draft.name, value: { kind: "secret", value: draft.value } };
    });
}

/** A Suggestion as reviewed before saving: checked or not, and edited. */
export type SuggestionDraft = {
  id: number;
  suggestion: ServiceSuggestion;
  isChecked: boolean;
  name: string;
  key: string;
  /** Once the user edits the key it stops following the name. */
  isKeyEdited: boolean;
  workingDirectory: string;
  commandLine: string;
  port: string;
  portCanChange: boolean;
};

export function suggestionDrafts(suggestions: ServiceSuggestion[]): SuggestionDraft[] {
  return suggestions.map((suggestion) => ({
    id: draftId(),
    suggestion,
    isChecked: suggestion.isRecommended,
    name: suggestion.name,
    key: suggestion.key,
    isKeyEdited: false,
    workingDirectory: suggestion.workingDirectory,
    commandLine: suggestion.commandLine,
    port: suggestion.port === null ? "" : String(suggestion.port),
    portCanChange: suggestion.portCanChange,
  }));
}

/** The checked drafts, to add. */
export function suggestionEdits(drafts: SuggestionDraft[]): SuggestionEdit[] {
  return drafts
    .filter((draft) => draft.isChecked)
    .map(({ name, key, workingDirectory, commandLine, port, portCanChange }) => ({
      name,
      key,
      workingDirectory,
      commandLine,
      port,
      portCanChange,
    }));
}

/** "./" for the Project's folder, else "./web". */
export function directoryLabel(workingDirectory: string): string {
  return workingDirectory === "" || workingDirectory === "." ? "./" : `./${workingDirectory}`;
}

/** Asks for a Project file, reads it, and opens its review sheet, or says why it can't be imported. */
export async function importProject() {
  const file = await commands.chooseProjectFile();
  if (!file) return;
  const result = await commands.readProjectFile(file);
  if (result.status === "ok") {
    showSheet({ kind: "importProject", file, preview: result.data });
  } else {
    showAlert({ kind: "importFailure", fileName: file.split("/").pop() ?? file, message: result.error.message });
  }
}

/** Asks where to save the Project's file, then writes it. */
export async function exportProject(projectId: Id) {
  const file = await commands.chooseExportFile(projectId);
  if (file.status === "error") return report(file);
  if (file.data) report(await commands.exportProject(projectId, file.data));
}

export async function setFavorite(project: Project, isFavorite: boolean) {
  report(await commands.setFavorite(project.id, isFavorite));
}

export async function setActiveProfile(projectId: Id, profileId: Id | null) {
  report(await commands.setActiveProfile(projectId, profileId));
}

export async function removeProject(projectId: Id) {
  report(await commands.removeProject(projectId));
}

/** Asks before removing the Service, naming the Services that depend on it. */
export async function requestRemoveService(serviceId: Id) {
  const relations = await commands.serviceRelations(serviceId);
  if (relations.status === "error") return report(relations);
  showAlert({ kind: "removeService", serviceId, dependents: relations.data.dependents });
}

export async function removeService(serviceId: Id) {
  report(await commands.removeService(serviceId));
}
