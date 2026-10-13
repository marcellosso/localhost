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

  it("says when there are no projects", () => {
    render(<Sidebar view={viewFromSnapshot({ projects: [], statuses: [], projectStates: [] })} selection={null} onSelect={() => {}} />);
    expect(screen.getByText("No projects yet.")).toBeTruthy();
  });
});
