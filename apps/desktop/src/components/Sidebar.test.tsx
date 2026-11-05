import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { viewFromSnapshot } from "../core";
import { machineView, project, service, status } from "../test/fixtures";
import { shownMachine } from "../machines";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("names each project with its Services' states, and selects on click", () => {
    const shop = project("Shop", 0, [service("Web"), service("Worker")]);
    const blog = project("Blog", 1);
    const view = viewFromSnapshot({
      projects: [shop, blog],
      statuses: [status(shop.services[0].id, "running"), status(shop.services[1].id, "failed")],
      projectStates: [{ projectId: shop.id, state: "failed" }],
    });
    const onSelect = vi.fn();
    render(<Sidebar view={view} selection={{ kind: "project", projectId: shop.id }} onSelect={onSelect} />);

    const rows = screen.getAllByRole("option");
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual([
      "Ports",
      "Shop, 1 of 2 running, 1 failed",
      "Blog, No services",
    ]);
    expect(rows[1].getAttribute("aria-selected")).toBe("true");
    // Focus stays on the list; Orca follows the selected row.
    expect(screen.getByRole("listbox").getAttribute("aria-activedescendant")).toBe(rows[1].id);

    fireEvent.click(rows[2]);
    expect(onSelect).toHaveBeenCalledWith({ kind: "project", projectId: blog.id });
    fireEvent.click(rows[0]);
    expect(onSelect).toHaveBeenCalledWith({ kind: "ports" });
  });

  it("filters projects by name or Key, and opens the first match with Return", () => {
    const shop = project("Acme Shop", 0);
    const blog = project("Blog", 1);
    const onSelect = vi.fn();
    render(
      <Sidebar view={viewFromSnapshot({ projects: [shop, blog], statuses: [], projectStates: [] })} selection={null} onSelect={onSelect} />,
    );
    const search = screen.getByRole("searchbox", { name: "Search projects" });

    fireEvent.change(search, { target: { value: "shop" } });
    expect(screen.getAllByRole("option").map((row) => row.getAttribute("aria-label"))).toEqual([
      "Ports",
      "Acme Shop, No services",
    ]);
    fireEvent.keyDown(search, { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith({ kind: "project", projectId: shop.id });

    fireEvent.change(search, { target: { value: "zzz" } });
    expect(screen.getByText("No projects match “zzz”.")).toBeTruthy();
  });

  it("shows each other Machine's Projects below this computer's, and why one isn't connected", () => {
    const web = service("Web");
    const shop = project("Shop", 0, [web]);
    const devbox = shownMachine(
      machineView("devbox", [shop], [status(web.id, "running")], { projectStates: [{ projectId: shop.id, state: "running" }] }),
    );
    const lab = shownMachine(
      machineView("lab", [], [], {
        state: { kind: "unavailable", detail: { problem: { kind: "hostKeyNotTrusted" }, isRetrying: false } },
        summary: "lab’s host key isn’t trusted yet. Connect once from a terminal with `ssh lab`, then try again.",
        isConnected: false,
        needsUser: true,
      }),
    );
    const onSelect = vi.fn();
    const empty = viewFromSnapshot({ projects: [], statuses: [], projectStates: [] });
    render(<Sidebar view={empty} machines={[devbox, lab]} selection={null} onSelect={onSelect} />);

    const rows = screen.getAllByRole("option");
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual(["Ports", "Shop, 1 of 1 running"]);
    fireEvent.click(rows[1]);
    expect(onSelect).toHaveBeenCalledWith({ kind: "machineProject", machineId: devbox.machine.id, projectId: shop.id });
    expect(screen.getByText(/lab’s host key isn’t trusted yet/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reconnect lab" })).toBeTruthy();
  });
});
