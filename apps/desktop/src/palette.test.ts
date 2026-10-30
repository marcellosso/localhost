import { describe, expect, it, vi } from "vitest";
import { viewFromSnapshot } from "./core";
import { emptyQueryResults, paletteItems, rankItems } from "./palette";
import { project, service, status } from "./test/fixtures";
import { currentUi, resetUi } from "./ui";

const commands = vi.hoisted(() => ({ fuzzyScores: vi.fn() }));
vi.mock("./bindings", () => ({ commands }));

const web = service("Web", 3000);
const api = service("API");
const shop = project("Shop", 0, [web, api]);
const blog = project("Blog", 1, [service("Hugo")]);
const view = viewFromSnapshot({ projects: [shop, blog], statuses: [status(web.id, "running", { urlToOpen: "http://localhost:3000" })], projectStates: [] });
const navigator = { selectPorts: vi.fn(), selectProject: vi.fn(), selectService: vi.fn() };

describe("palette", () => {
  it("offers the selection's actions first, then jumps; the rest only when typed for", () => {
    const items = paletteItems(view, shop, { kind: "service", serviceId: web.id }, navigator);
    const shown = emptyQueryResults(items).map((item) => [item.title, item.shortcut ?? ""]);
    expect(shown).toEqual([
      ["Restart Web", "Ctrl+R"],
      ["Stop Web", "Ctrl+."],
      ["Open Web in Browser", "Ctrl+Shift+O"],
      ["Open Web in Terminal", ""],
      ["Open Web in Editor", ""],
      ["Web", "Ctrl+Alt+1"],
      ["API", "Ctrl+Alt+2"],
      ["Start All in Shop", "Ctrl+Shift+R"],
      ["Stop All in Shop", "Ctrl+Shift+."],
      ["Open Shop in Editor", ""],
      ["Open Shop in Terminal", ""],
      ["Open Shop in File Manager", ""],
      ["Blog", "Ctrl+2"],
      ["Ports", "Ctrl+0"],
      ["New Project…", "Ctrl+N"],
      ["Keyboard Shortcuts", "Ctrl+/"],
    ]);
    expect(items.some((item) => item.title === "Start Hugo" && item.id.startsWith("other:"))).toBe(true);
  });

  it("ranks matches with the core's scores and the items' boosts, dropping the rest", async () => {
    const items = paletteItems(view, shop, { kind: "all" }, navigator);
    commands.fuzzyScores.mockImplementation(async (_query: string, texts: string[]) =>
      texts.map((text) => (text.startsWith("Hugo") ? 10 : text.startsWith("Start Hugo") ? 11 : null)),
    );
    const ranked = await rankItems("hugo", items);
    // The jump (boost 2) outranks the action (boost 0) it scored less than.
    expect(ranked.map((item) => item.title)).toEqual(["Hugo", "Start Hugo"]);

    ranked[0].run();
    expect(navigator.selectService).toHaveBeenCalledWith(blog.id, blog.services[0].id);
  });

  it("opens Connect Agents when typed for", () => {
    resetUi();
    const item = paletteItems(view, shop, { kind: "all" }, navigator).find((item) => item.title === "Connect Agents…")!;
    expect(item.id.startsWith("other:")).toBe(true);
    expect(item.keywords).toContain("mcp");
    item.run();
    expect(currentUi().sheet).toEqual({ kind: "connectAgents" });
  });
});
