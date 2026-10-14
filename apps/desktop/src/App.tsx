import { useEffect, useMemo, useState } from "react";
import { runLaunchChecks } from "./actions";
import { commands, events, type Reveal } from "./bindings";
import { Alerts } from "./components/Alerts";
import { Announcer } from "./components/Announcer";
import { CommandPalette } from "./components/CommandPalette";
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
import type { CoreView } from "./core";
import type { Navigator } from "./palette";
import { useWindowShortcuts } from "./shortcuts";
import { startCore, useCore } from "./store";
import { showSheet, useUi } from "./ui";

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

  return <MainWindow view={core.view} />;
}

/** Sidebar → Services (or Ports) → log, the sheets and alerts over them, and the keyboard. */
function MainWindow({ view }: { view: CoreView }) {
  const [sidebarSelection, setSidebarSelection] = useState<SidebarSelection | null>(null);
  const [selection, setSelection] = useState<ServiceSelection>(allServices);
  const { isPaletteOpen, sheet, alerts } = useUi();
  // Under a sheet, an alert or the palette, the window behind can't be reached, by screen readers either.
  const isBehindModal = isPaletteOpen || sheet !== null || alerts.length > 0;

  const isPorts = sidebarSelection?.kind === "ports";
  const selectedId = sidebarSelection?.kind === "project" ? sidebarSelection.projectId : null;
  const selected = isPorts ? undefined : (view.projects.find((project) => project.id === selectedId) ?? view.projects[0]);
  const select = (next: SidebarSelection) => {
    setSidebarSelection(next);
    setSelection(allServices);
  };
  const navigator = useMemo<Navigator>(
    () => ({
      selectPorts: () => {
        setSidebarSelection({ kind: "ports" });
        setSelection(allServices);
      },
      selectProject: (projectId) => {
        setSidebarSelection({ kind: "project", projectId });
        setSelection(allServices);
      },
      selectService: (projectId, serviceId) => {
        setSidebarSelection({ kind: "project", projectId });
        setSelection({ kind: "service", serviceId });
      },
    }),
    [],
  );
  useWindowShortcuts({ view, selected, selection, setSelection, navigator });
  useReveal(navigator);

  return (
    <div className="app">
      <div className="contents" inert={isBehindModal}>
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
            <section className="emptyColumn" data-column="list">
              <EmptyState icon={<FolderIcon size={40} />} title="No Projects" description="Add a project folder to get started.">
                <button className="button accent" onClick={() => showSheet({ kind: "addProject" })}>
                  Add Project…
                </button>
                <button className="button" onClick={() => void importProject()}>
                  Import Project…
                </button>
              </EmptyState>
            </section>
            <section className="emptyLog" data-column="detail">
              <EmptyState
                icon={<LinesIcon size={40} />}
                title="No Service Selected"
                description="Select a service to see its output."
              />
            </section>
          </>
        )}
      </div>
      <Sheets view={view} onSelectProject={navigator.selectProject} onSelectService={navigator.selectService} />
      {isPaletteOpen && <CommandPalette view={view} selected={selected} selection={selection} navigator={navigator} />}
      <Alerts view={view} />
      <Announcer view={view} selectedProjectId={selected?.id ?? null} />
    </div>
  );
}

/** Goes where the tray asks: once as the window opens, then on each reveal while it is open. */
function useReveal(navigator: Navigator) {
  useEffect(() => {
    const go = (reveal: Reveal | null) => {
      if (reveal?.kind === "ports") navigator.selectPorts();
      else if (reveal?.serviceId) navigator.selectService(reveal.projectId, reveal.serviceId);
      else if (reveal) navigator.selectProject(reveal.projectId);
    };
    void commands.takeReveal().then(go);
    const unlisten = events.reveal.listen((event) => go(event.payload));
    return () => void unlisten.then((stop) => stop());
  }, [navigator]);
}
