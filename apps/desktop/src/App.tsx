import { useEffect, useState } from "react";
import type { Id } from "./bindings";
import { ProjectServices } from "./components/ProjectServices";
import { Sidebar } from "./components/Sidebar";
import { startCore, useCore } from "./store";

export function App() {
  const core = useCore();
  const [selectedId, setSelectedId] = useState<Id | null>(null);

  useEffect(() => {
    void startCore();
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
  return (
    <div className="app">
      <Sidebar view={view} selectedId={selected?.id ?? null} onSelect={setSelectedId} />
      {selected ? (
        <ProjectServices project={selected} view={view} />
      ) : (
        <main className="message">
          <h1>No projects</h1>
          <p>Projects you add appear here.</p>
        </main>
      )}
    </div>
  );
}
