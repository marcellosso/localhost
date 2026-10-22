import { afterEach, describe, expect, it, vi } from "vitest";
import { openFolder, start } from "./actions";
import { currentUi, resetUi } from "./ui";

const commands = vi.hoisted(() => ({ settings: vi.fn(), openFolder: vi.fn(), startService: vi.fn() }));
vi.mock("./bindings", () => ({ commands }));

const folder = { kind: "project", projectId: "p" } as const;

describe("actions", () => {
  afterEach(resetUi);

  it("asks for an editor before the first Open in Editor, then opens", async () => {
    commands.settings.mockResolvedValue({ editorCommand: null, automaticPortMove: true });
    commands.openFolder.mockResolvedValue({ status: "ok", data: null });
    await openFolder(folder, "editor");
    expect(commands.openFolder).not.toHaveBeenCalled();
    const [alert] = currentUi().alerts;
    expect(alert.kind).toBe("settings");

    commands.settings.mockResolvedValue({ editorCommand: "code", automaticPortMove: true });
    if (alert.kind === "settings") alert.forEditor?.();
    await vi.waitFor(() => expect(commands.openFolder).toHaveBeenCalledWith(folder, "editor"));
  });

  it("reports a failure in the core's words", async () => {
    const message = "The working directory /gone does not exist.";
    commands.startService.mockResolvedValue({ status: "error", error: { error: { kind: "workingDirectoryMissing", detail: "/gone" }, message } });
    await start("s");
    expect(currentUi().alerts).toEqual([{ kind: "error", message }]);
  });
});
