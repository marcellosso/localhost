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
    render(<Sidebar view={view} selectedId={shop.id} onSelect={onSelect} />);

    const rows = screen.getAllByRole("option");
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual(["Shop, Failed", "Blog, No services"]);
    expect(rows[0].getAttribute("aria-selected")).toBe("true");

    fireEvent.click(rows[1]);
    expect(onSelect).toHaveBeenCalledWith(blog.id);
  });

  it("says when there are no projects", () => {
    render(<Sidebar view={viewFromSnapshot({ projects: [], statuses: [], projectStates: [] })} selectedId={null} onSelect={() => {}} />);
    expect(screen.getByText("No projects yet.")).toBeTruthy();
  });
});
