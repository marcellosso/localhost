import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { viewFromSnapshot } from "../core";
import { project, service } from "../test/fixtures";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("names each project with its state, and selects on click", () => {
    const shop = project("Shop", 0, [service("Web")]);
    const blog = project("Blog", 1);
    const view = viewFromSnapshot({
      projects: [shop, blog],
      statuses: [],
      projectStates: [{ projectId: shop.id, state: "failed" }],
    });
    const onSelect = vi.fn();
    render(<Sidebar view={view} selection={{ kind: "project", projectId: shop.id }} onSelect={onSelect} />);

    const rows = screen.getAllByRole("option");
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual([
      "Ports",
      "Shop, Failed",
      "Blog, No services",
    ]);
    expect(rows[1].getAttribute("aria-selected")).toBe("true");

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
});
