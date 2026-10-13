import { useEffect, useState } from "react";
import type { Id } from "./bindings";
import { LogPane } from "./components/LogPane";
import { ServicesColumn } from "./components/ServicesColumn";
import { Sidebar } from "./components/Sidebar";
import { runPerfMeasurement } from "./perf";
import { allServices, type ServiceSelection } from "./selection";
import { startCore, useCore } from "./store";

export function App() {
  const core = useCore();
  const [selectedId, setSelectedId] = useState<Id | null>(null);
  const [selection, setSelection] = useState<ServiceSelection>(allServices);

  useEffect(() => {
    void startCore().then(runPerfMeasurement);
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
  const selected = view.projects.find((project) => project.id === selectedId) ?? view.projects[0];
  const selectProject = (id: Id) => {
    setSelectedId(id);
    setSelection(allServices);
  };
  return (
    <div className="app">
      <Sidebar view={view} selectedId={selected?.id ?? null} onSelect={selectProject} />
      {selected ? (
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
    </div>
  );
}
