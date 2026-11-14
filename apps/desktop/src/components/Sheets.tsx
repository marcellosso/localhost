import { useEffect } from "react";
import type { Id } from "../bindings";
import { type CoreView, findService } from "../core";
import type { MachineRef } from "../editing";
import { useMachines } from "../machines";
import { closeSheet, type Sheet, useUi } from "../ui";
import { ConnectAgentsSheet } from "./ConnectAgents";
import { EnvironmentForm } from "./EnvironmentForm";
import { ImportProjectForm } from "./ImportProjectForm";
import { ProjectForm } from "./ProjectForm";
import { RemoteFolderPicker } from "./RemoteFolderPicker";
import { ServiceForm } from "./ServiceForm";
import { DetectServicesForm } from "./SuggestionReview";
import { Welcome } from "./Welcome";

type Props = {
  view: CoreView;
  /** Selects what was just added; on another Machine with `machine`. */
  onSelectProject: (projectId: Id, machine?: MachineRef) => void;
  onSelectService: (projectId: Id, serviceId: Id, machine?: MachineRef) => void;
};

/**
 * The editor on screen, if any, and the remote folder picker over it. One
 * whose Project or Service went away closes, as does one for a Machine that
 * was removed.
 */
export function Sheets({ view, onSelectProject, onSelectService }: Props) {
  const { sheet } = useUi();
  const machines = useMachines();
  const machine = sheet && "machine" in sheet ? sheet.machine : undefined;
  const machineView = machine ? machines.find((shown) => shown.machine.id === machine.id)?.view : view;
  // This computer's are selected as before; a Machine's say which Machine.
  const selectProject: Props["onSelectProject"] = (projectId) =>
    machine ? onSelectProject(projectId, machine) : onSelectProject(projectId);
  const selectService: Props["onSelectService"] = (projectId, serviceId) =>
    machine ? onSelectService(projectId, serviceId, machine) : onSelectService(projectId, serviceId);
  const content = sheet && machineView && sheetContent(sheet, machineView, machine, selectProject, selectService);
  const isGone = sheet !== null && !content;
  useEffect(() => {
    if (isGone) closeSheet();
  }, [isGone]);
  return (
    <>
      {content}
      <RemoteFolderPicker />
    </>
  );
}

function sheetContent(
  sheet: Sheet,
  view: CoreView,
  machine: MachineRef | undefined,
  onSelectProject: Props["onSelectProject"],
  onSelectService: Props["onSelectService"],
) {
  const project = (id: Id) => view.projects.find((project) => project.id === id);
  switch (sheet.kind) {
    case "addProject":
      return <ProjectForm machine={machine} onAdded={(added) => onSelectProject(added.id)} />;
    case "editProject": {
      const editing = project(sheet.projectId);
      return editing ? <ProjectForm key={editing.id} editing={editing} machine={machine} /> : null;
    }
    case "addService": {
      const target = project(sheet.projectId);
      return target ? (
        <ServiceForm project={target} machine={machine} onAdded={(added) => onSelectService(target.id, added.id)} />
      ) : null;
    }
    case "editService": {
      const found = findService(view, sheet.serviceId);
      return found ? (
        <ServiceForm
          key={sheet.serviceId}
          project={found.project}
          editing={found.project.services[found.index]}
          machine={machine}
        />
      ) : null;
    }
    case "detectServices": {
      const target = project(sheet.projectId);
      return target ? (
        <DetectServicesForm
          project={target}
          machine={machine}
          onAdded={(serviceId) => onSelectService(target.id, serviceId)}
        />
      ) : null;
    }
    case "environment": {
      const target = project(sheet.projectId);
      return target ? <EnvironmentForm key={target.id} project={target} machine={machine} /> : null;
    }
    case "importProject":
      return (
        <ImportProjectForm
          file={sheet.file}
          preview={sheet.preview}
          machine={machine}
          onImported={(imported) => onSelectProject(imported.id)}
        />
      );
    case "connectAgents":
      return <ConnectAgentsSheet onClose={closeSheet} />;
    case "welcome":
      return <Welcome />;
  }
}
