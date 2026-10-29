import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateNotice } from "./UpdateNotice";

const commands = vi.hoisted(() => ({
  putOffUpdate: vi.fn(async () => null),
  downloadUpdate: vi.fn(async () => null),
  installUpdate: vi.fn(async () => null),
  openUrl: vi.fn(async () => ({ status: "ok", data: null })),
}));
vi.mock("../bindings", () => ({ commands }));

const notesUrl = "https://github.com/marcellosso/localhost/releases/tag/v0.5.0";

describe("UpdateNotice", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows nothing without an update", () => {
    const { container } = render(<UpdateNotice update={{ kind: "none" }} />);
    expect(container.innerHTML).toBe("");
  });

  it("offers a newer version: Update Now downloads it, Later puts it off, the notes open in the browser", () => {
    render(
      <UpdateNotice update={{ kind: "available", version: "0.5.0", notesUrl, canInstall: true, problem: null }} />,
    );
    expect(screen.getByRole("region", { name: "Update" }).textContent).toContain("localhost 0.5.0 is available");

    fireEvent.click(screen.getByRole("button", { name: "Update Now" }));
    expect(commands.downloadUpdate).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Later" }));
    expect(commands.putOffUpdate).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Release Notes" }));
    expect(commands.openUrl).toHaveBeenCalledWith(notesUrl);
  });

  it("sends a package manager's users to the release instead of installing", () => {
    render(
      <UpdateNotice update={{ kind: "available", version: "0.5.0", notesUrl, canInstall: false, problem: null }} />,
    );
    expect(screen.queryByRole("button", { name: "Update Now" })).toBeNull();
    expect(screen.getByText("Install it with your package manager.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    expect(commands.openUrl).toHaveBeenCalledWith(notesUrl);
  });

  it("says why a download failed and offers it again", () => {
    const problem = "The download failed: connection reset. Try again.";
    render(<UpdateNotice update={{ kind: "available", version: "0.5.0", notesUrl, canInstall: true, problem }} />);
    expect(screen.getByRole("alert").textContent).toBe(problem);
    fireEvent.click(screen.getByRole("button", { name: "Try Again" }));
    expect(commands.downloadUpdate).toHaveBeenCalled();
  });

  it("shows the download's progress, then restarts to install", () => {
    const { rerender } = render(
      <UpdateNotice update={{ kind: "downloading", version: "0.5.0", notesUrl, progress: 0.4 }} />,
    );
    const progress = screen.getByRole("progressbar", { name: "Downloading localhost 0.5.0" }) as HTMLProgressElement;
    expect(progress.value).toBe(0.4);

    rerender(<UpdateNotice update={{ kind: "ready", version: "0.5.0", notesUrl }} />);
    expect(screen.getByText("Restarting stops running services first.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Restart to Update" }));
    expect(commands.installUpdate).toHaveBeenCalled();
  });
});
