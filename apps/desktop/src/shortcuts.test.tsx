import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CommandPalette } from "./components/CommandPalette";
import { viewFromSnapshot } from "./core";
import type { ServiceSelection } from "./selection";
import { adjacentRow, useWindowShortcuts } from "./shortcuts";
import { project, service, status } from "./test/fixtures";
import { currentUi, resetUi, showSheet, useUi } from "./ui";

const commands = vi.hoisted(() => ({
  startService: vi.fn(),
  restartService: vi.fn(),
  stopService: vi.fn(),
  startAll: vi.fn(),
  fuzzyScores: vi.fn(),
  quitApp: vi.fn(),
}));
vi.mock("./bindings", () => ({ commands }));
const closeWindow = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: closeWindow }) }));

const web = service("Web", 3000);
const api = service("API");
const shop = project("Shop", 0, [web, api]);
const blog = project("Blog", 1);
const view = viewFromSnapshot({ projects: [shop, blog], statuses: [status(web.id, "running")], projectStates: [] });
const navigator = { selectPorts: vi.fn(), selectProject: vi.fn(), selectService: vi.fn() };

/** The window's keyboard over a sidebar and a Services list, like the app's columns. */
function Window({ initial }: { initial: ServiceSelection }) {
  const [selection, setSelection] = useState(initial);
  const { isPaletteOpen } = useUi();
  useWindowShortcuts({ view, selected: shop, selection, setSelection, navigator });
  const option = (row: ServiceSelection, name: string) => (
    <li role="option" aria-selected={JSON.stringify(row) === JSON.stringify(selection)} onClick={() => setSelection(row)}>
      {name}
    </li>
  );
  return (
    <>
      <nav data-column="sidebar">
        <ul role="listbox" aria-label="Projects" tabIndex={0} />
      </nav>
      <section data-column="list">
        <ul role="listbox" aria-label="Services" tabIndex={0}>
          {option({ kind: "all" }, "All services")}
          {option({ kind: "service", serviceId: web.id }, "Web")}
          {option({ kind: "service", serviceId: api.id }, "API")}
        </ul>
      </section>
      <section data-column="detail">
        <div role="log" tabIndex={0} aria-label="Log" />
      </section>
      {isPaletteOpen && <CommandPalette view={view} selected={shop} selection={selection} navigator={navigator} />}
    </>
  );
}

const press = (init: KeyboardEventInit) => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, ...init })));
const ok = { status: "ok", data: null } as const;

describe("window shortcuts", () => {
  beforeEach(() => {
    Object.values(commands).forEach((command) => command.mockResolvedValue(ok));
    commands.fuzzyScores.mockImplementation(async (query: string, texts: string[]) =>
      texts.map((text) => (text.toLowerCase().includes(query.toLowerCase()) ? 1 : null)),
    );
  });
  afterEach(() => {
    act(resetUi);
    vi.clearAllMocks();
  });

  it("restarts a running Service with Ctrl+R, and stops it with Ctrl+.", () => {
    render(<Window initial={{ kind: "service", serviceId: web.id }} />);
    press({ code: "KeyR", key: "r", ctrlKey: true });
    expect(commands.restartService).toHaveBeenCalledWith(web.id);
    press({ code: "Period", key: ".", ctrlKey: true });
    expect(commands.stopService).toHaveBeenCalledWith(web.id);
  });

  it("selects Projects with Ctrl+1–9, Services with Ctrl+Alt+1–9, Ports with Ctrl+0", () => {
    render(<Window initial={{ kind: "all" }} />);
    press({ code: "Digit2", key: "2", ctrlKey: true });
    expect(navigator.selectProject).toHaveBeenCalledWith(blog.id);
    press({ code: "Digit2", key: "™", ctrlKey: true, altKey: true });
    expect(navigator.selectService).toHaveBeenCalledWith(shop.id, api.id);
    press({ code: "Digit0", key: "0", ctrlKey: true });
    expect(navigator.selectPorts).toHaveBeenCalled();
  });

  it("does nothing under a sheet", () => {
    render(<Window initial={{ kind: "all" }} />);
    act(() => showSheet({ kind: "addProject" }));
    press({ code: "KeyR", key: "R", ctrlKey: true, shiftKey: true });
    expect(commands.startAll).not.toHaveBeenCalled();
  });

  it("quits with Ctrl+Q and closes the window with Ctrl+W, even under a sheet", () => {
    render(<Window initial={{ kind: "all" }} />);
    act(() => showSheet({ kind: "addProject" }));
    press({ code: "KeyQ", key: "q", ctrlKey: true });
    expect(commands.quitApp).toHaveBeenCalled();
    press({ code: "KeyW", key: "w", ctrlKey: true });
    expect(closeWindow).toHaveBeenCalled();
  });

  it("moves through the list with ↑/↓ and across the columns with ←/→ and Escape", () => {
    render(<Window initial={{ kind: "all" }} />);
    const services = screen.getByRole("listbox", { name: "Services" });
    services.focus();
    press({ key: "ArrowDown" });
    expect(screen.getByRole("option", { name: "Web" }).getAttribute("aria-selected")).toBe("true");
    press({ key: "ArrowRight" });
    expect(document.activeElement).toBe(screen.getByRole("log"));
    press({ key: "Escape" });
    expect(document.activeElement).toBe(services);
    press({ key: "ArrowLeft" });
    expect(document.activeElement).toBe(screen.getByRole("listbox", { name: "Projects" }));
  });

  it("steps through the rows with Ctrl+[ and Ctrl+], stopping at the ends", () => {
    expect(adjacentRow(shop, { kind: "all" }, 1)).toEqual({ kind: "service", serviceId: web.id });
    expect(adjacentRow(shop, { kind: "all" }, -1)).toEqual({ kind: "all" });
    expect(adjacentRow(shop, { kind: "service", serviceId: api.id }, 1)).toEqual({ kind: "service", serviceId: api.id });
  });

  it("opens the palette with Ctrl+K, runs the chosen item, and gives focus back", async () => {
    render(<Window initial={{ kind: "all" }} />);
    const services = screen.getByRole("listbox", { name: "Services" });
    services.focus();
    press({ code: "KeyK", key: "k", ctrlKey: true });
    const field = screen.getByRole("combobox");
    expect(document.activeElement).toBe(field);

    await act(async () => fireEvent.change(field, { target: { value: "blog" } }));
    const results = screen.getByRole("listbox", { name: "Results" });
    expect(within(results).getByRole("option", { selected: true }).textContent).toContain("Blog");
    await act(async () => fireEvent.keyDown(field, { key: "Enter" }));
    expect(navigator.selectProject).toHaveBeenCalledWith(blog.id);
    expect(currentUi().isPaletteOpen).toBe(false);
    await act(() => new Promise(requestAnimationFrame));
    expect(document.activeElement).toBe(services);
  });

  it("closes the palette with Escape, without running anything", () => {
    render(<Window initial={{ kind: "all" }} />);
    press({ code: "KeyK", key: "k", ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "Escape" });
    expect(currentUi().isPaletteOpen).toBe(false);
    expect(navigator.selectProject).not.toHaveBeenCalled();
  });
});
