import { beforeEach, describe, expect, it, vi } from "vitest";
import { currentUi, resetUi } from "./ui";
import { checkForUpdates } from "./updates";

const commands = vi.hoisted(() => ({ checkForUpdates: vi.fn() }));
vi.mock("./bindings", () => ({ commands }));

describe("checkForUpdates", () => {
  beforeEach(() => resetUi());

  it("says so when localhost is up to date", async () => {
    commands.checkForUpdates.mockResolvedValue({ kind: "upToDate", version: "0.4.0" });
    await checkForUpdates();
    expect(currentUi().alerts).toEqual([{ kind: "upToDate", version: "0.4.0" }]);
  });

  it("says why it couldn't check", async () => {
    const message = "localhost couldn’t check for updates: offline. Check your connection and try again.";
    commands.checkForUpdates.mockResolvedValue({ kind: "failed", message });
    await checkForUpdates();
    expect(currentUi().alerts).toEqual([{ kind: "error", message }]);
  });

  it("leaves a newer version to the notice", async () => {
    commands.checkForUpdates.mockResolvedValue({ kind: "available", version: "0.5.0" });
    await checkForUpdates();
    expect(currentUi().alerts).toEqual([]);
  });
});
