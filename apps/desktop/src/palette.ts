import { openFolder, openUrl, restart, start, startAll, stop, stopAll } from "./actions";
import { commands, type Id, type Project, type Service, type ServiceState } from "./bindings";
import { type CoreView, serviceStatus } from "./core";
import { machineRef } from "./editing";
import { importProject, setFavorite } from "./editors";
import type { ShownMachine } from "./machines";
import { projectShortcut, SHORTCUTS, type Shortcut, serviceShortcut, shortcutLabel } from "./keyboard";
import { tildePath } from "./paths";
import type { ServiceSelection } from "./selection";
import { isActive } from "./status";
import { showChecklistAgain } from "./onboarding";
import { showAlert, showSheet } from "./ui";
import { checkForUpdates } from "./updates";

// What the command palette (Ctrl+K) offers: jumps to Projects and Services,
// and actions on them.

/** Where the palette's jumps go; the window's selection lives in App. */
export type Navigator = {
  selectPorts: () => void;
  selectProject: (projectId: Id) => void;
  selectService: (projectId: Id, serviceId: Id) => void;
};

export type PaletteIcon =
  | { kind: "symbol"; name: "folder" | "service" | "play" | "stop" | "restart" | "browser" | "terminal" | "editor" | "ports" | "new" | "keyboard" | "agents" | "help" }
  /** A Service: its colour (by position) and state glyph. */
  | { kind: "service"; index: number; state: ServiceState };

export type PaletteItem = {
  /** Stable across rebuilds, e.g. `start:<service id>`, so the selection survives typing. Ids starting `other:` only show when typed for. */
  id: string;
  title: string;
  subtitle?: string;
  icon: PaletteIcon;
  /** What does the same outside the palette, e.g. "Ctrl+R". */
  shortcut?: string;
  /** Extra words it matches, e.g. the Service's Key and its Project's name. */
  keywords?: string;
  /** Ranks it above equally good matches (jumps over actions, the selection over the rest). */
  boost?: number;
  run: () => void;
};

const label = (shortcut: Shortcut | null) => (shortcut ? shortcutLabel(shortcut) : undefined);

/** How many items an empty query shows: the ones about the selection, and the jumps. */
export const EMPTY_QUERY_LIMIT = 30;

/**
 * Everything the palette offers for the current state. An empty query shows
 * the first ones: the selected Service's actions, the selected Project's
 * Services and actions, the other Projects, Ports and New Project. The rest
 * (ids starting `other:`) are reachable by typing.
 */
export function paletteItems(
  view: CoreView,
  selected: Project | undefined,
  selection: ServiceSelection,
  navigator: Navigator,
): PaletteItem[] {
  const items: PaletteItem[] = [];
  const selectedService =
    selection.kind === "service" ? selected?.services.find((service) => service.id === selection.serviceId) : undefined;
  if (selected && selectedService) items.push(...serviceActions(view, selectedService, selected, true));
  if (selected) {
    items.push(...selected.services.map((service, index) => serviceJump(view, service, selected, index, true, navigator)));
    items.push(...projectActions(view, selected, true));
  }
  view.projects.forEach((project, index) => {
    if (project.id !== selected?.id) items.push(projectJump(project, index, navigator));
  });
  items.push({
    id: "ports",
    title: "Ports",
    subtitle: "Every listening port on this computer",
    icon: { kind: "symbol", name: "ports" },
    shortcut: label(SHORTCUTS.ports),
    keywords: "listeners",
    run: navigator.selectPorts,
  });
  items.push({
    id: "new-project",
    title: "New Project…",
    icon: { kind: "symbol", name: "new" },
    shortcut: label(SHORTCUTS.newProject),
    keywords: "add",
    run: () => showSheet({ kind: "addProject" }),
  });
  items.push({
    id: "keyboard-shortcuts",
    title: "Keyboard Shortcuts",
    icon: { kind: "symbol", name: "keyboard" },
    shortcut: label(SHORTCUTS.keyboardShortcuts),
    keywords: "help keys cheat sheet",
    run: () => showAlert({ kind: "keyboardShortcuts" }),
  });
  items.push({
    id: "connect-agents",
    title: "Connect Agents…",
    icon: { kind: "symbol", name: "agents" },
    keywords: "mcp claude cursor codex agent skill path terminal cli",
    run: () => showSheet({ kind: "connectAgents" }),
  });
  items.push({
    id: "other:show-welcome",
    title: "Show Welcome…",
    icon: { kind: "symbol", name: "help" },
    keywords: "help onboarding introduction tour first run",
    run: () => showSheet({ kind: "welcome" }),
  });
  items.push({
    id: "other:show-checklist",
    title: "Show Get Started Checklist",
    icon: { kind: "symbol", name: "help" },
    keywords: "help onboarding get started first run",
    run: showChecklistAgain,
  });
  items.push({
    id: "other:check-for-updates",
    title: "Check for Updates…",
    icon: { kind: "symbol", name: "new" },
    keywords: "update upgrade version release",
    run: () => void checkForUpdates(),
  });
  // Only reachable by typing: other Projects' Services and actions.
  for (const project of view.projects) {
    if (project.id === selected?.id) continue;
    items.push(...project.services.map((service, index) => serviceJump(view, service, project, index, false, navigator)));
    items.push(...projectActions(view, project, false));
    for (const service of project.services) items.push(...serviceActions(view, service, project, false));
  }
  if (selected) {
    for (const service of selected.services) {
      if (service.id !== selectedService?.id) items.push(...serviceActions(view, service, selected, false));
    }
  }
  return items;
}

/** The items an empty query shows. */
export function emptyQueryResults(items: PaletteItem[]): PaletteItem[] {
  const firstOther = items.findIndex((item) => item.id.startsWith("other:"));
  return items.slice(0, firstOther < 0 ? items.length : firstOther).slice(0, EMPTY_QUERY_LIMIT);
}

/** The items matching `query`, best first, ranked by the core's FuzzyMatch. */
export async function rankItems(query: string, items: PaletteItem[]): Promise<PaletteItem[]> {
  if (!query.trim()) return emptyQueryResults(items);
  const texts = items.map((item) => `${item.title} ${item.subtitle ?? ""} ${item.keywords ?? ""}`);
  const scores = await commands.fuzzyScores(query, texts);
  return items
    .map((item, offset) => ({ item, offset, score: scores[offset] }))
    .filter((ranked): ranked is { item: PaletteItem; offset: number; score: number } => ranked.score !== null)
    .map((ranked) => ({ ...ranked, score: ranked.score + (ranked.item.boost ?? 0) }))
    .sort((a, b) => b.score - a.score || a.offset - b.offset)
    .slice(0, 50)
    .map((ranked) => ranked.item);
}

function projectJump(project: Project, index: number, navigator: Navigator): PaletteItem {
  return {
    id: `project:${project.id}`,
    title: project.name,
    subtitle: tildePath(project.path),
    icon: { kind: "symbol", name: "folder" },
    shortcut: label(projectShortcut(index)),
    keywords: `${project.key} go project`,
    boost: 3,
    run: () => navigator.selectProject(project.id),
  };
}

function serviceJump(
  view: CoreView,
  service: Service,
  project: Project,
  index: number,
  isInSelectedProject: boolean,
  navigator: Navigator,
): PaletteItem {
  return {
    id: `${isInSelectedProject ? "" : "other:"}service:${service.id}`,
    title: service.name,
    subtitle: `${project.name} · ${service.commandLine}`,
    icon: { kind: "service", index, state: serviceStatus(view, service.id).state },
    shortcut: isInSelectedProject ? label(serviceShortcut(index)) : undefined,
    keywords: `${service.key} ${project.key} go service`,
    boost: isInSelectedProject ? 4 : 2,
    run: () => navigator.selectService(project.id, service.id),
  };
}

function serviceActions(view: CoreView, service: Service, project: Project, isSelected: boolean): PaletteItem[] {
  const prefix = isSelected ? "" : "other:";
  const status = serviceStatus(view, service.id);
  const items: PaletteItem[] = [];
  const item = (action: string, title: string, icon: PaletteIcon, shortcut: Shortcut | null, run: () => void) =>
    items.push({
      id: `${prefix}${action}:${service.id}`,
      title,
      subtitle: project.name,
      icon,
      shortcut: isSelected ? label(shortcut) : undefined,
      keywords: `${service.key} ${project.name} ${project.key}`,
      boost: isSelected ? 2 : 0,
      run,
    });
  const symbol = (name: Extract<PaletteIcon, { kind: "symbol" }>["name"]): PaletteIcon => ({ kind: "symbol", name });
  switch (status.state) {
    case "starting":
    case "running":
      item("restart", `Restart ${service.name}`, symbol("restart"), SHORTCUTS.startOrRestart, () => void restart(service.id));
      item("stop", `Stop ${service.name}`, symbol("stop"), SHORTCUTS.stop, () => void stop(service.id));
      if (status.urlToOpen) {
        item("open-url", `Open ${service.name} in Browser`, symbol("browser"), SHORTCUTS.openUrl, () => void openUrl(service.id));
      }
      break;
    case "stopping":
      break;
    default:
      item("start", `Start ${service.name}`, symbol("play"), SHORTCUTS.startOrRestart, () => void start(service.id));
  }
  const folder = { kind: "service", serviceId: service.id } as const;
  item("terminal", `Open ${service.name} in Terminal`, symbol("terminal"), null, () => void openFolder(folder, "terminal"));
  item("editor", `Open ${service.name} in Editor`, symbol("editor"), null, () => void openFolder(folder, "editor"));
  return items;
}

function projectActions(view: CoreView, project: Project, isSelected: boolean): PaletteItem[] {
  const prefix = isSelected ? "" : "other:";
  const items: PaletteItem[] = [];
  const item = (action: string, title: string, name: Extract<PaletteIcon, { kind: "symbol" }>["name"], shortcut: Shortcut | null, run: () => void) =>
    items.push({
      id: `${prefix}${action}:${project.id}`,
      title,
      icon: { kind: "symbol", name },
      shortcut: isSelected ? label(shortcut) : undefined,
      keywords: project.key,
      boost: isSelected ? 1 : 0,
      run,
    });
  if (project.services.length > 0) {
    item("start-all", `Start All in ${project.name}`, "play", SHORTCUTS.startAll, () => void startAll(project.id));
  }
  if (project.services.some((service) => isActive(serviceStatus(view, service.id).state))) {
    item("stop-all", `Stop All in ${project.name}`, "stop", SHORTCUTS.stopAll, () => void stopAll(project.id));
  }
  const folder = { kind: "project", projectId: project.id } as const;
  item("project-editor", `Open ${project.name} in Editor`, "editor", null, () => void openFolder(folder, "editor"));
  item("project-terminal", `Open ${project.name} in Terminal`, "terminal", null, () => void openFolder(folder, "terminal"));
  item("project-files", `Open ${project.name} in File Manager`, "folder", null, () => void openFolder(folder, "fileManager"));
  return items;
}

/**
 * Another Machine's actions: for its selected Project, the same as for one
 * here (first, as an empty query shows them), made there; and for every
 * Machine, New Project… and Import Project… there, found by typing. While
 * its Projects can't be changed, an edit says why instead.
 */
export function machinePaletteItems(
  machines: ShownMachine[],
  selected: { machine: ShownMachine; project: Project; selection: ServiceSelection } | null,
): { first: PaletteItem[]; rest: PaletteItem[] } {
  const first: PaletteItem[] = [];
  const symbol = (name: Extract<PaletteIcon, { kind: "symbol" }>["name"]): PaletteIcon => ({ kind: "symbol", name });
  /** Runs `run`, or says why the Machine's Projects can't be changed now. */
  const editing = (shown: ShownMachine, run: () => void) => () =>
    shown.editProblem ? showAlert({ kind: "error", message: shown.editProblem }) : run();
  if (selected) {
    const { machine: shown, project } = selected;
    const machine = machineRef(shown);
    const machineId = machine.id;
    const view = shown.view;
    const where = `${project.name} on ${machine.name}`;
    const item = (id: string, title: string, icon: PaletteIcon, shortcut: Shortcut | null, run: () => void, subtitle = where) =>
      first.push({ id: `machine:${id}:${project.id}`, title, subtitle, icon, shortcut: label(shortcut), keywords: project.key, boost: 2, run });
    const selection = selected.selection;
    const service =
      selection.kind === "service" ? project.services.find((one) => one.id === selection.serviceId) : undefined;
    if (service && shown.isConnected) {
      const status = serviceStatus(view, service.id);
      if (status.state === "starting" || status.state === "running") {
        item(`restart:${service.id}`, `Restart ${service.name}`, symbol("restart"), SHORTCUTS.startOrRestart, () => void restart(service.id, machineId));
        item(`stop:${service.id}`, `Stop ${service.name}`, symbol("stop"), SHORTCUTS.stop, () => void stop(service.id, machineId));
        if (status.urlToOpen) {
          item(`open-url:${service.id}`, `Open ${service.name} in Browser`, symbol("browser"), SHORTCUTS.openUrl, () => void openUrl(service.id, machineId));
        }
      } else if (status.state !== "stopping") {
        item(`start:${service.id}`, `Start ${service.name}`, symbol("play"), SHORTCUTS.startOrRestart, () => void start(service.id, null, machineId));
      }
    }
    if (shown.isConnected && project.services.length > 0) {
      item("start-all", `Start All in ${project.name}`, symbol("play"), SHORTCUTS.startAll, () => void startAll(project.id, machineId));
    }
    if (shown.isConnected && project.services.some((one) => isActive(serviceStatus(view, one.id).state))) {
      item("stop-all", `Stop All in ${project.name}`, symbol("stop"), SHORTCUTS.stopAll, () => void stopAll(project.id, machineId));
    }
    const subtitle = shown.editProblem ?? where;
    const projectId = project.id;
    item("new-service", "New Service…", symbol("new"), SHORTCUTS.newService, editing(shown, () => showSheet({ kind: "addService", projectId, machine })), subtitle);
    item("edit-project", "Edit Project…", symbol("folder"), null, editing(shown, () => showSheet({ kind: "editProject", projectId, machine })), subtitle);
    item("environment", "Edit Environment…", symbol("folder"), null, editing(shown, () => showSheet({ kind: "environment", projectId, machine })), subtitle);
    item("detect", "Detect Services…", symbol("service"), null, editing(shown, () => showSheet({ kind: "detectServices", projectId, machine })), subtitle);
    item(
      "favorite",
      project.isFavorite ? `Unfavorite ${project.name}` : `Favorite ${project.name}`,
      symbol("folder"),
      null,
      editing(shown, () => void setFavorite(project, !project.isFavorite, machineId)),
      subtitle,
    );
    item("remove", `Remove ${project.name}…`, symbol("stop"), null, editing(shown, () => showAlert({ kind: "removeProject", projectId, machine })), subtitle);
  }
  const rest: PaletteItem[] = machines.flatMap((shown) => {
    const machine = machineRef(shown);
    const subtitle = shown.editProblem ?? undefined;
    return [
      {
        id: `other:machine-new-project:${machine.id}`,
        title: `New Project on ${machine.name}…`,
        subtitle,
        icon: symbol("new"),
        keywords: "add machine remote",
        run: editing(shown, () => showSheet({ kind: "addProject", machine })),
      },
      {
        id: `other:machine-import:${machine.id}`,
        title: `Import Project on ${machine.name}…`,
        subtitle,
        icon: symbol("new"),
        keywords: "add machine remote file yaml",
        run: editing(shown, () => void importProject(machine)),
      },
    ];
  });
  return { first, rest };
}
