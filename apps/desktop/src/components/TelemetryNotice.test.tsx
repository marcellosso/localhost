import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadTelemetryNotice } from "../telemetry";
import { TelemetryNotice } from "./TelemetryNotice";

const commands = vi.hoisted(() => ({
  telemetryNoticePending: vi.fn(async () => true),
  acknowledgeTelemetryNotice: vi.fn(async () => null),
  openUrl: vi.fn(async () => ({ status: "ok", data: null })),
}));
vi.mock("../bindings", () => ({ commands }));
const ui = vi.hoisted(() => ({ showAlert: vi.fn() }));
vi.mock("../ui", () => ui);

async function launch(pending: boolean) {
  commands.telemetryNoticePending.mockResolvedValue(pending);
  await act(loadTelemetryNotice);
}

describe("TelemetryNotice", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows nothing once the notice was seen, or when this build sends nothing", async () => {
    await launch(false);
    const { container } = render(<TelemetryNotice />);
    expect(container.innerHTML).toBe("");
  });

  it("says what is sent; OK dismisses it for good", async () => {
    await launch(true);
    render(<TelemetryNotice />);
    const notice = screen.getByRole("region", { name: "Anonymous usage data" });
    expect(notice.textContent).toContain("Settings › General");
    expect(notice.textContent).toContain("DO_NOT_TRACK=1");

    fireEvent.click(screen.getByRole("button", { name: "Privacy" }));
    expect(commands.openUrl).toHaveBeenCalledWith("https://www.getlocalhost.app/privacy");
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(commands.acknowledgeTelemetryNotice).toHaveBeenCalled();
    expect(screen.queryByRole("region", { name: "Anonymous usage data" })).toBeNull();
  });

  it("Settings… opens General and dismisses it", async () => {
    await launch(true);
    render(<TelemetryNotice />);
    fireEvent.click(screen.getByRole("button", { name: "Settings…" }));
    expect(ui.showAlert).toHaveBeenCalledWith({ kind: "settings", section: "general" });
    expect(commands.acknowledgeTelemetryNotice).toHaveBeenCalled();
    expect(screen.queryByRole("region", { name: "Anonymous usage data" })).toBeNull();
  });
});
