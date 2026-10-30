import { useEffect } from "react";
import type { Id } from "../bindings";
import { type CoreView, findService } from "../core";
import { closeSheet, type Sheet, useUi } from "../ui";
import { ConnectAgentsSheet } from "./ConnectAgents";
import { EnvironmentForm } from "./EnvironmentForm";
import { ImportProjectForm } from "./ImportProjectForm";
import { ProjectForm } from "./ProjectForm";
import { ServiceForm } from "./ServiceForm";
import { DetectServicesForm } from "./SuggestionReview";

type Props = {
  view: CoreView;
  /** Selects what was just added. */
  onSelectProject: (projectId: Id) => void;
  onSelectService: (projectId: Id, serviceId: Id) => void;
};

/** The editor on screen, if any. One whose Project or Service went away closes. */
export function Sheets({ view, onSelectProject, onSelectService }: Props) {
  const { sheet } = useUi();
  const content = sheet && sheetContent(sheet, view, onSelectProject, onSelectService);
  const isGone = sheet !== null && content === null;
  useEffect(() => {
    if (isGone) closeSheet();
  }, [isGone]);
  return content;
}

function sheetContent(sheet: Sheet, view: CoreView, onSelectProject: Props["onSelectProject"], onSelectService: Props["onSelectService"]) {
  const project = (id: Id) => view.projects.find((project) => project.id === id);
  switch (sheet.kind) {
    case "addProject":
      return <ProjectForm onAdded={(added) => onSelectProject(added.id)} />;
    case "editProject": {
      const editing = project(sheet.projectId);
      return editing ? <ProjectForm key={editing.id} editing={editing} /> : null;
    }
    case "addService": {
      const target = project(sheet.projectId);
      return target ? <ServiceForm project={target} onAdded={(added) => onSelectService(target.id, added.id)} /> : null;
    }
    case "editService": {
      const found = findService(view, sheet.serviceId);
      return found ? <ServiceForm key={sheet.serviceId} project={found.project} editing={found.project.services[found.index]} /> : null;
    }
    case "detectServices": {
      const target = project(sheet.projectId);
      return target ? <DetectServicesForm project={target} onAdded={(serviceId) => onSelectService(target.id, serviceId)} /> : null;
    }
    case "environment": {
      const target = project(sheet.projectId);
      return target ? <EnvironmentForm key={target.id} project={target} /> : null;
    }
    case "importProject":
      return <ImportProjectForm file={sheet.file} preview={sheet.preview} onImported={(imported) => onSelectProject(imported.id)} />;
    case "connectAgents":
      return <ConnectAgentsSheet onClose={closeSheet} />;
  }
}
