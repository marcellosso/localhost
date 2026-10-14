import { useEffect, useState } from "react";
import { runLaunchChecks } from "./actions";
import { Alerts } from "./components/Alerts";
import { EmptyState } from "./components/EmptyState";
import { Sheets } from "./components/Sheets";
import { FolderIcon, LinesIcon } from "./components/Icons";
import { LogPane } from "./components/LogPane";
import { PortsView } from "./components/PortsView";
import { ProjectActions, ServicesColumn } from "./components/ServicesColumn";
import { Sidebar } from "./components/Sidebar";
import { Toolbar } from "./components/Toolbar";
import { importProject } from "./editors";
import { loadHome, tildePath } from "./paths";
import { runPerfMeasurement } from "./perf";
import { allServices, type ServiceSelection, type SidebarSelection } from "./selection";
import { loadSettings } from "./settings";
import { startCore, useCore } from "./store";
import { showSheet } from "./ui";

let started: Promise<void> | null = null;

/** Subscribes to the core, then asks the launch questions: once per window, though effects may run twice. */
function start(): Promise<void> {
  started ??= Promise.all([loadSettings(), loadHome(), startCore()]).then(async () => {
    await runLaunchChecks();
    await runPerfMeasurement();
  });
  return started;
}

export function App() {
  const core = useCore();
  const [sidebarSelection, setSidebarSelection] = useState<SidebarSelection | null>(null);
  const [selection, setSelection] = useState<ServiceSelection>(allServices);

  useEffect(() => {
    void start();
  }, []);

  if (core.phase === "loading") return null;
  if (core.phase === "failed") {
    return (
      <main className="failed" role="alert">
        <EmptyState icon={<FolderIcon size={40} />} title="localhost couldn’t open its data" description={core.message} />
      </main>
    );
  }

  const { view } = core;
  const isPorts = sidebarSelection?.kind === "ports";
  const selectedId = sidebarSelection?.kind === "project" ? sidebarSelection.projectId : null;
  const selected = isPorts ? undefined : (view.projects.find((project) => project.id === selectedId) ?? view.projects[0]);
  const select = (next: SidebarSelection) => {
    setSidebarSelection(next);
    setSelection(allServices);
  };
  return (
    <div className="app">
      <Sidebar
        view={view}
        selection={isPorts ? sidebarSelection : selected ? { kind: "project", projectId: selected.id } : null}
        onSelect={select}
      />
      {isPorts ? (
        <>
          <Toolbar title="Ports" />
          <PortsView view={view} />
        </>
      ) : selected ? (
        <>
          <Toolbar
            title={selected.name}
            subtitle={tildePath(selected.path)}
            actions={<ProjectActions project={selected} view={view} />}
          />
          <ServicesColumn project={selected} view={view} selection={selection} onSelect={setSelection} />
          <LogPane project={selected} view={view} selection={selection} />
        </>
      ) : (
        <>
          <Toolbar title="localhost" />
          <section className="emptyColumn">
            <EmptyState icon={<FolderIcon size={40} />} title="No Projects" description="Add a project folder to get started.">
              <button className="button accent" onClick={() => showSheet({ kind: "addProject" })}>
                Add Project…
              </button>
              <button className="button" onClick={() => void importProject()}>
                Import Project…
              </button>
            </EmptyState>
          </section>
          <section className="emptyLog">
            <EmptyState
              icon={<LinesIcon size={40} />}
              title="No Service Selected"
              description="Select a service to see its output."
            />
          </section>
        </>
      )}
      <Sheets
        view={view}
        onSelectProject={(projectId) => select({ kind: "project", projectId })}
        onSelectService={(projectId, serviceId) => {
          setSidebarSelection({ kind: "project", projectId });
          setSelection({ kind: "service", serviceId });
        }}
      />
      <Alerts view={view} />
    </div>
  );
}
