import { describe, expect, it } from "vitest";
import { applyEvent, serviceState, viewFromSnapshot } from "./core";
import { project, service, status } from "./test/fixtures";

describe("the core view", () => {
  it("keeps the core's project order: sort order, then name", () => {
    const view = viewFromSnapshot({
      projects: [project("Zed", 1), project("Blog", 2), project("Api", 1)],
      statuses: [],
      projectStates: [],
    });
    expect(view.projects.map((p) => p.name)).toEqual(["Api", "Zed", "Blog"]);
  });

  it("adds, updates and removes projects", () => {
    const shop = project("Shop", 0);
    let view = viewFromSnapshot({ projects: [shop], statuses: [], projectStates: [] });

    const blog = project("Blog", 1);
    view = applyEvent(view, { kind: "library", event: { kind: "projectAdded", project: blog, origin: "mainWindow" } });
    expect(view.projects.map((p) => p.name)).toEqual(["Shop", "Blog"]);

    const renamed = { ...shop, name: "Store", sortOrder: 2 };
    view = applyEvent(view, { kind: "library", event: { kind: "projectUpdated", project: renamed, origin: "cli" } });
    expect(view.projects.map((p) => p.name)).toEqual(["Blog", "Store"]);

    view = applyEvent(view, { kind: "projectState", event: { projectId: blog.id, state: null } });
    view = applyEvent(view, { kind: "library", event: { kind: "projectRemoved", id: blog.id, origin: "menuBar" } });
    expect(view.projects.map((p) => p.name)).toEqual(["Store"]);
    expect(view.projectStates.has(blog.id)).toBe(false);
  });

  it("tracks service statuses and project states", () => {
    const web = service("Web", 3000);
    const shop = project("Shop", 0, [web]);
    let view = viewFromSnapshot({ projects: [shop], statuses: [], projectStates: [{ projectId: shop.id, state: "stopped" }] });
    expect(serviceState(view, web.id)).toBe("stopped");

    view = applyEvent(view, { kind: "status", event: status(web.id, "running") });
    view = applyEvent(view, { kind: "projectState", event: { projectId: shop.id, state: "running" } });
    expect(serviceState(view, web.id)).toBe("running");
    expect(view.projectStates.get(shop.id)).toBe("running");
  });
});
