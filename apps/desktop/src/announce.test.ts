import { describe, expect, it } from "vitest";
import { currentStates, settledAnnouncements } from "./announce";
import type { StatusView } from "./bindings";
import { viewFromSnapshot } from "./core";
import { project, service, status } from "./test/fixtures";

describe("settledAnnouncements", () => {
  const shop = project("Shop", 0, [service("Web"), service("Worker")]);
  const blog = project("Blog", 1, [service("Site")]);
  const [web, worker] = shop.services;
  const site = blog.services[0];
  const view = (statuses: StatusView[]) =>
    viewFromSnapshot({ projects: [shop, blog], statuses, projectStates: [] });

  it("announces Services that settle, naming the Project when it isn't the selected one", () => {
    const states = currentStates(view([]));
    const announcements = settledAnnouncements(
      states,
      view([
        status(web.id, "running"),
        status(worker.id, "failed", { failureMessage: "Command not found." }),
        status(site.id, "exited"),
      ]),
      shop.id,
    );
    expect(announcements).toEqual([
      { text: "Web, Running", isUrgent: false },
      { text: "Worker, Failed, Command not found.", isUrgent: true },
      { text: "Blog, Site, Exited, code 0", isUrgent: false },
    ]);
  });

  it("stays quiet for Starting, Stopping and states already announced", () => {
    const states = currentStates(view([status(web.id, "running")]));
    expect(settledAnnouncements(states, view([status(web.id, "running"), status(worker.id, "starting")]), shop.id)).toEqual([]);
    expect(settledAnnouncements(states, view([status(web.id, "stopping"), status(worker.id, "starting")]), shop.id)).toEqual([]);
    expect(settledAnnouncements(states, view([status(web.id, "stopped"), status(worker.id, "running")]), null)).toEqual([
      { text: "Shop, Web, Stopped", isUrgent: false },
      { text: "Shop, Worker, Running", isUrgent: false },
    ]);
  });
});
