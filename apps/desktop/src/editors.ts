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
import type { MachineRef } from "./editing";
import { chooseOnMachine } from "./remoteFolders";
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
  return {
    id: draftId(),
    name: "",
    value: "",
    isSecret: false,
    storedSecretName: null,
    isValueKnown: true,
    isRevealed: false,
  };
}

/** Plain variables first, then Secrets, each sorted by name. */
export function variableDrafts(
  variables: Variables | undefined,
): VariableDraft[] {
  if (!variables) return [];
  const plain = Object.entries(variables.values)
    .toSorted(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([name, value]) => ({
      ...newVariableDraft(),
      name,
      value: value ?? "",
    }));
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
  return {
    values: service.environment,
    secretNames: service.secretNames ?? [],
  };
}

/** What to save; empty rows are left out. */
export function variableEdits(drafts: VariableDraft[]): VariableEdit[] {
  return drafts
    .filter((draft) => draft.name !== "" || draft.value !== "")
    .map((draft): VariableEdit => {
      if (!draft.isSecret)
        return {
          name: draft.name,
          value: { kind: "plain", value: draft.value },
        };
      if (draft.storedSecretName !== null && !draft.isValueKnown) {
        return {
          name: draft.name,
          value: { kind: "storedSecret", previousName: draft.storedSecretName },
        };
      }
      return {
        name: draft.name,
        value: { kind: "secret", value: draft.value },
      };
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
  /** The virtual environment scanning found, while the user keeps it; empty for none. */
  pythonEnvironment: string;
  /** The drafts it starts after; only the checked ones are saved. */
  startsAfter: number[];
  /** The Keys of the Project's Services it starts after. */
  startsAfterServices: string[];
  /** The drafts merged into this one, to split it again. */
  mergedFrom: SuggestionDraft[];
};

function suggestionDraft(suggestion: ServiceSuggestion): SuggestionDraft {
  return {
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
    pythonEnvironment: suggestion.pythonEnvironment ?? "",
    startsAfter: [],
    startsAfterServices: [],
    mergedFrom: [],
  };
}

export function suggestionDrafts(
  suggestions: ServiceSuggestion[],
): SuggestionDraft[] {
  return suggestions.map(suggestionDraft);
}

/** The checked drafts, to add, each starting after the checked drafts and the Project's Services it picked. */
export function suggestionEdits(drafts: SuggestionDraft[]): SuggestionEdit[] {
  const checked = drafts.filter((draft) => draft.isChecked);
  const keys = new Map(checked.map((draft) => [draft.id, draft.key]));
  return checked.map(
    ({
      name,
      key,
      workingDirectory,
      commandLine,
      port,
      portCanChange,
      pythonEnvironment,
      startsAfter,
      startsAfterServices,
    }) => ({
      name,
      key,
      workingDirectory,
      commandLine,
      port,
      portCanChange,
      pythonEnvironment,
      dependencyKeys: [
        ...startsAfterServices,
        ...startsAfter.flatMap((id) => keys.get(id) ?? []),
      ],
    }),
  );
}

/** A draft as the Suggestion it now is, with the user's edits, to merge it. */
export function editedSuggestion(draft: SuggestionDraft): ServiceSuggestion {
  const port = /^\d+$/.test(draft.port.trim())
    ? Number(draft.port.trim())
    : null;
  return {
    ...draft.suggestion,
    name: draft.name,
    key: draft.key,
    workingDirectory: draft.workingDirectory,
    commandLine: draft.commandLine,
    port,
    portCanChange: draft.portCanChange,
    pythonEnvironment: draft.pythonEnvironment || null,
  };
}

/**
 * Replaces the drafts `ids` with one, checked, for `merged` (the core's merge
 * of them), where the first of them was. Drafts that started after any of
 * them start after the merged one.
 */
export function mergingDrafts(
  drafts: SuggestionDraft[],
  ids: number[],
  merged: ServiceSuggestion,
): SuggestionDraft[] {
  const parts = drafts.filter((draft) => ids.includes(draft.id));
  if (parts.length === 0) return drafts;
  const draft: SuggestionDraft = {
    ...suggestionDraft({ ...merged, isRecommended: true }),
    isChecked: true,
    startsAfter: unique(parts.flatMap((part) => part.startsAfter)).filter(
      (id) => !ids.includes(id),
    ),
    startsAfterServices: [
      ...new Set(parts.flatMap((part) => part.startsAfterServices)),
    ],
    mergedFrom: parts,
  };
  const position = drafts.indexOf(parts[0]);
  const rest = drafts.map((other) => ({
    ...other,
    startsAfter: replacing(other.startsAfter, ids, [draft.id]),
  }));
  return [...rest.slice(0, position), draft, ...rest.slice(position)].filter(
    (other) => !ids.includes(other.id),
  );
}

/** The drafts running one Compose service each that `draft` (running several) could split into. */
export function containerDrafts(
  drafts: SuggestionDraft[],
  draft: SuggestionDraft,
): SuggestionDraft[] {
  const services = draft.suggestion.composeServices ?? [];
  if (services.length < 2) return [];
  return drafts.filter((other) => {
    const own = other.suggestion.composeServices ?? [];
    return (
      other.id !== draft.id &&
      own.length === 1 &&
      services.includes(own[0]) &&
      other.suggestion.foundIn === draft.suggestion.foundIn &&
      other.workingDirectory === draft.workingDirectory
    );
  });
}

/** Whether `draft` can be split again: merged here, or one running a whole Compose file found with its containers. */
export function canSplit(
  drafts: SuggestionDraft[],
  draft: SuggestionDraft,
): boolean {
  return (
    draft.mergedFrom.length > 0 || containerDrafts(drafts, draft).length > 0
  );
}

/**
 * Undoes a merge, or checks the one-per-container drafts instead of one
 * running the whole Compose file (which they start after what it started
 * after). Drafts that started after it start after all of them.
 */
export function splittingDraft(
  drafts: SuggestionDraft[],
  id: number,
): SuggestionDraft[] {
  const draft = drafts.find((draft) => draft.id === id);
  if (draft === undefined) return drafts;
  if (draft.mergedFrom.length > 0) {
    const parts = draft.mergedFrom.map((part) => ({
      ...part,
      isChecked: true,
    }));
    const partIds = parts.map((part) => part.id);
    return drafts.flatMap((other) =>
      other.id === id
        ? parts
        : [
            {
              ...other,
              startsAfter: replacing(other.startsAfter, [id], partIds),
            },
          ],
    );
  }
  const partIds = containerDrafts(drafts, draft).map((part) => part.id);
  if (partIds.length === 0) return drafts;
  return drafts.map((other) => {
    if (other.id === id) return { ...other, isChecked: false };
    const startsAfter = replacing(other.startsAfter, [id], partIds);
    return partIds.includes(other.id)
      ? {
          ...other,
          isChecked: true,
          startsAfter: unique([...startsAfter, ...draft.startsAfter]),
          startsAfterServices: [
            ...new Set([
              ...other.startsAfterServices,
              ...draft.startsAfterServices,
            ]),
          ],
        }
      : { ...other, startsAfter };
  });
}

/** Whether `draft` starts after `other`, directly or through other checked drafts: then `other` can't start after it. */
export function draftStartsAfter(
  drafts: SuggestionDraft[],
  draft: SuggestionDraft,
  other: SuggestionDraft,
): boolean {
  const byId = new Map(drafts.filter((d) => d.isChecked).map((d) => [d.id, d]));
  const seen = new Set<number>();
  const visit = (current: SuggestionDraft): boolean =>
    current.startsAfter.some((id) => {
      if (id === other.id) return true;
      const next = byId.get(id);
      if (next === undefined || seen.has(id)) return false;
      seen.add(id);
      return visit(next);
    });
  return visit(draft);
}

function unique(ids: number[]): number[] {
  return [...new Set(ids)];
}

/** `ids` with any of `removed` replaced by `added`, once each. */
function replacing(
  ids: number[],
  removed: number[],
  added: number[],
): number[] {
  if (!ids.some((id) => removed.includes(id))) return ids;
  return unique(ids.flatMap((id) => (removed.includes(id) ? added : [id])));
}

/** "./" for the Project's folder, else "./web". */
export function directoryLabel(workingDirectory: string): string {
  return workingDirectory === "" || workingDirectory === "."
    ? "./"
    : `./${workingDirectory}`;
}

/**
 * Asks for a Project file, reads it, and opens its review sheet, or says why
 * it can't be imported. On `machine`, the file is that Machine's, picked
 * with the remote folder picker and read there.
 */
export async function importProject(machine: MachineRef | null = null) {
  const file = machine ? await chooseOnMachine(machine, { isFile: true }) : await commands.chooseProjectFile();
  if (!file) return;
  const result = machine ? await commands.machineReadProjectFile(machine.id, file) : await commands.readProjectFile(file);
  if (result.status === "ok") {
    showSheet({ kind: "importProject", file, preview: result.data, machine: machine ?? undefined });
  } else {
    showAlert({
      kind: "importFailure",
      fileName: file.split("/").pop() ?? file,
      message: result.error.message,
    });
  }
}

/** Asks where to save the Project's file, then writes it. */
export async function exportProject(projectId: Id) {
  const file = await commands.chooseExportFile(projectId);
  if (file.status === "error") return report(file);
  if (file.data) report(await commands.exportProject(projectId, file.data));
}

// Each on this computer, or with `machineId` on that Machine.

export async function setFavorite(project: Project, isFavorite: boolean, machineId: Id | null = null) {
  report(
    await (machineId
      ? commands.machineSetFavorite(machineId, project.id, isFavorite)
      : commands.setFavorite(project.id, isFavorite)),
  );
}

export async function setActiveProfile(projectId: Id, profileId: Id | null, machineId: Id | null = null) {
  report(
    await (machineId
      ? commands.machineSetActiveProfile(machineId, projectId, profileId)
      : commands.setActiveProfile(projectId, profileId)),
  );
}

export async function removeProject(projectId: Id, machineId: Id | null = null) {
  report(await (machineId ? commands.machineRemoveProject(machineId, projectId) : commands.removeProject(projectId)));
}

/** Asks before removing the Service, naming the Services that depend on it. */
export async function requestRemoveService(serviceId: Id, machine: MachineRef | null = null) {
  const relations = machine
    ? await commands.machineServiceRelations(machine.id, serviceId)
    : await commands.serviceRelations(serviceId);
  if (relations.status === "error") return report(relations);
  showAlert({
    kind: "removeService",
    serviceId,
    dependents: relations.data.dependents,
    machine: machine ?? undefined,
  });
}

export async function removeService(serviceId: Id, machineId: Id | null = null) {
  report(await (machineId ? commands.machineRemoveService(machineId, serviceId) : commands.removeService(serviceId)));
}
