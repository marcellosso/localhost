import { useEffect, useState } from "react";
import { runLaunchChecks } from "./actions";
import { Alerts } from "./components/Alerts";
import { LogPane } from "./components/LogPane";
import { PortsView } from "./components/PortsView";
import { ServicesColumn } from "./components/ServicesColumn";
import { Sidebar } from "./components/Sidebar";
import { runPerfMeasurement } from "./perf";
import { allServices, type ServiceSelection, type SidebarSelection } from "./selection";
import { startCore, useCore } from "./store";

let started: Promise<void> | null = null;

/** Subscribes to the core, then asks the launch questions: once per window, though effects may run twice. */
function start(): Promise<void> {
  started ??= startCore().then(async () => {
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
      <main className="message" role="alert">
        <h1>localhost couldn't open its data</h1>
        <p>{core.message}</p>
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
        <PortsView view={view} />
      ) : selected ? (
        <>
          <ServicesColumn project={selected} view={view} selection={selection} onSelect={setSelection} />
          <LogPane project={selected} view={view} selection={selection} />
        </>
      ) : (
        <main className="message">
          <h1>No projects</h1>
          <p>Projects you add appear here.</p>
        </main>
      )}
      <Alerts view={view} />
    </div>
  );
}
