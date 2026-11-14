import { commands, type ExecutionTarget, type Id, type ProjectFolder } from "./bindings";
import type { ShownMachine } from "./machines";
import { chooseOnMachine } from "./remoteFolders";
import { thisMachine } from "./targets";

// Where an editor saves: this computer, or another Machine, whose Projects
// are changed there. The sheets are the same; `editorCommands` sends their
// calls to one or the other, and a Machine's folders are picked with the
// remote folder picker, as no system dialog here can browse them.

/** A Machine an editor works on, by id and by the name it is shown under. */
export type MachineRef = { id: Id; name: string };

export function machineRef(shown: ShownMachine): MachineRef {
  return { id: shown.machine.id, name: shown.machine.name };
}

/** " on devbox" after an editor's title, or nothing on this computer. */
export function onMachine(machine: MachineRef | null | undefined): string {
  return machine ? ` on ${machine.name}` : "";
}

/** The editors' calls, on this computer (`null`) or on `machine`. */
export function editorCommands(machine: MachineRef | null) {
  if (!machine) {
    return {
      suggestedProjectKey: commands.suggestedProjectKey,
      suggestedServiceKey: commands.suggestedServiceKey,
      addProject: commands.addProject,
      editProject: commands.editProject,
      scanFolder: commands.scanFolder,
      scanProject: commands.scanProject,
      addService: commands.addService,
      editService: commands.editService,
      addServices: commands.addServices,
      serviceRelations: commands.serviceRelations,
      detectPythonEnvironment: commands.detectPythonEnvironment,
      saveEnvironment: commands.saveEnvironment,
      secretValue: commands.secretValue,
      importProject: commands.importProject,
      chooseFolder: commands.chooseFolder,
      chooseWorkingDirectory: commands.chooseWorkingDirectory,
    };
  }
  const id = machine.id;
  return {
    suggestedProjectKey: (name: string, excluding: Id | null) => commands.machineSuggestedProjectKey(id, name, excluding),
    suggestedServiceKey: (projectId: Id, name: string, excluding: Id | null) =>
      commands.machineSuggestedServiceKey(id, projectId, name, excluding),
    addProject: (...[name, key, folder, services]: Parameters<typeof commands.addProject>) =>
      commands.machineAddProject(id, name, key, folder.path, services),
    editProject: (...[projectId, name, key, folder]: Parameters<typeof commands.editProject>) =>
      commands.machineEditProject(id, projectId, name, key, folder.path),
    scanFolder: (folder: ProjectFolder) => commands.machineScanFolder(id, folder.path),
    scanProject: (projectId: Id) => commands.machineScanProject(id, projectId),
    addService: (...[projectId, service]: Parameters<typeof commands.addService>) =>
      commands.machineAddService(id, projectId, service),
    editService: (...[serviceId, service]: Parameters<typeof commands.editService>) =>
      commands.machineEditService(id, serviceId, service),
    addServices: (...[projectId, services]: Parameters<typeof commands.addServices>) =>
      commands.machineAddServices(id, projectId, services),
    serviceRelations: (serviceId: Id) => commands.machineServiceRelations(id, serviceId),
    detectPythonEnvironment: (...[projectId, workingDirectory, commandLine]: Parameters<typeof commands.detectPythonEnvironment>) =>
      commands.machineDetectPythonEnvironment(id, projectId, workingDirectory, commandLine),
    saveEnvironment: (...[projectId, variables, profiles, active]: Parameters<typeof commands.saveEnvironment>) =>
      commands.machineSaveEnvironment(id, projectId, variables, profiles, active),
    secretValue: (...[name, layer]: Parameters<typeof commands.secretValue>) => commands.machineSecretValue(id, name, layer),
    importProject: (...[file, folder, choice]: Parameters<typeof commands.importProject>) =>
      commands.machineImportProject(id, file, folder.path, choice),
    /** A folder of the Machine, as one this app's editors take. */
    chooseFolder: async (_target: ExecutionTarget, start: string | null): Promise<ProjectFolder | null> => {
      const path = await chooseOnMachine(machine, { start });
      return path === null ? null : { target: thisMachine, path };
    },
    /** A folder of the Machine, relative to the Project's. */
    chooseWorkingDirectory: async (project: ProjectFolder): Promise<string | null> => {
      const path = await chooseOnMachine(machine, { start: project.path });
      return path === null ? null : commands.relativeFolder(path, project.path);
    },
  };
}

export type EditorCommands = ReturnType<typeof editorCommands>;
