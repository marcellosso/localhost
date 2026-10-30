import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { onAnnouncement } from "../announce";
import type { AgentStatus, CommandLineStatus } from "../bindings";
import { ConnectAgents, ConnectAgentsSheet } from "./ConnectAgents";

const commands = vi.hoisted(() => ({
  agentStatuses: vi.fn(),
  connectAgent: vi.fn(),
  disconnectAgent: vi.fn(),
  commandLineStatus: vi.fn(),
  addCommandToPath: vi.fn(),
  removeCommandFromPath: vi.fn(),
  openUrl: vi.fn(async () => ({ status: "ok", data: null })),
}));
vi.mock("../bindings", () => ({ commands }));

const ok = <T,>(data: T) => ({ status: "ok", data }) as const;
const failure = (message: string) =>
  ({ status: "error", error: { error: { kind: "agentConfigUnsupported", detail: { path: "", reason: "" } }, message } }) as const;

function agent(client: AgentStatus["client"], name: string, changes: Partial<AgentStatus> = {}): AgentStatus {
  return {
    client,
    name,
    connection: "notConnected",
    summary: `${name} isn’t connected.`,
    configPath: `/home/me/.${client}/config.json`,
    entry: `"localhost": { "command": "/usr/bin/localhost", "args": ["mcp"] }`,
    skillPath: `/home/me/.${client}/skills/localhost-mcp`,
    viaCommand: null,
    skillSharedFrom: null,
    configuredCommand: null,
    isSkillInstalled: false,
    backupPath: null,
    ...changes,
  };
}

const claude = agent("claudeCode", "Claude Code");
const cursor = agent("cursor", "Cursor", {
  connection: "connected",
  summary: "Cursor runs this app’s localhost.",
  configuredCommand: "/usr/bin/localhost",
  backupPath: "/home/me/.cursor/mcp.json.localhost-backup",
});
const codex = agent("codex", "Codex", { connection: "notInstalled", summary: "Codex isn’t installed." });

function commandLine(state: CommandLineStatus["state"], summary: string): CommandLineStatus {
  return { state, path: "/home/me/.local/bin/localhost", command: "/opt/localhost/localhost", summary };
}

function group(name: string) {
  return within(screen.getByRole("group", { name }));
}

describe("ConnectAgents", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    commands.agentStatuses.mockResolvedValue([claude, cursor, codex]);
    commands.commandLineStatus.mockResolvedValue(commandLine("notAdded", "Terminals don’t find localhost."));
  });

  it("shows each agent's state and summary, with the action it allows", async () => {
    render(<ConnectAgents />);
    await screen.findByRole("group", { name: "Claude Code" });

    expect(group("Claude Code").getByText("Not connected")).toBeTruthy();
    expect(group("Claude Code").getByText("Claude Code isn’t connected.")).toBeTruthy();
    expect(group("Claude Code").getByRole("button", { name: "Connect Claude Code" }).className).toContain("accent");
    expect(group("Cursor").getByText("Connected")).toBeTruthy();
    expect(group("Cursor").getByRole("button", { name: "Disconnect Cursor" }).className).not.toContain("accent");
    expect(group("Codex").getByText("Not installed")).toBeTruthy();
    expect(group("Codex").queryAllByRole("button")).toEqual([]);
  });

  it("shows the agents before the command-line status arrives", async () => {
    commands.commandLineStatus.mockReturnValue(new Promise(() => {}));
    render(<ConnectAgents />);
    await screen.findByRole("group", { name: "Claude Code" });
    expect(screen.getByText("Looking for the localhost command…")).toBeTruthy();
  });

  it("tells what Connect writes, behind a disclosure", async () => {
    render(<ConnectAgents />);
    const toggle = await screen.findByRole("button", { name: "What changes for Cursor" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");

    const changes = group("Cursor");
    expect(changes.getByText(cursor.configPath)).toBeTruthy();
    expect(changes.getByText(cursor.entry)).toBeTruthy();
    expect(changes.getByText(cursor.skillPath)).toBeTruthy();
    expect(changes.getByText(cursor.backupPath!)).toBeTruthy();
    expect(changes.queryByText(/Now runs/)).toBeNull();
  });

  it("says when a command makes the change and where a shared skill is", async () => {
    commands.agentStatuses.mockResolvedValue([
      { ...claude, viaCommand: "claude mcp add-json --scope user localhost" },
      { ...cursor, skillSharedFrom: "Claude Code", skillPath: "/home/me/.claude/skills/localhost-mcp/SKILL.md" },
    ]);
    render(<ConnectAgents />);
    fireEvent.click(await screen.findByRole("button", { name: "What changes for Claude Code" }));
    expect(group("Claude Code").getByText("claude mcp add-json --scope user localhost")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "What changes for Cursor" }));
    expect(group("Cursor").getByText(/Uses the skill Claude Code has at/)).toBeTruthy();
  });

  it("names the other command an agent runs before an update", async () => {
    const outdated = { ...cursor, connection: "needsUpdate" as const, configuredCommand: "/home/me/.cargo/bin/localhost" };
    commands.agentStatuses.mockResolvedValue([outdated]);
    render(<ConnectAgents />);
    fireEvent.click(await screen.findByRole("button", { name: "What changes for Cursor" }));
    expect(group("Cursor").getByText("/home/me/.cargo/bin/localhost")).toBeTruthy();
  });

  it("connects an agent, updates its row and announces it", async () => {
    const spoken: string[] = [];
    const stop = onAnnouncement((announcement) => spoken.push(announcement.text));
    let finish: (value: unknown) => void = () => {};
    commands.connectAgent.mockReturnValue(new Promise((resolve) => (finish = resolve)));
    render(<ConnectAgents />);

    const connect = await screen.findByRole("button", { name: "Connect Claude Code" });
    fireEvent.click(connect);
    expect(commands.connectAgent).toHaveBeenCalledWith("claudeCode");
    expect((connect as HTMLButtonElement).disabled).toBe(true);

    finish(ok({ ...claude, connection: "connected", summary: "Claude Code runs this app’s localhost." }));
    await waitFor(() => expect(group("Claude Code").getByText("Connected")).toBeTruthy());
    expect(group("Claude Code").getByText("Claude Code runs this app’s localhost.")).toBeTruthy();
    expect(group("Claude Code").getByRole("button", { name: "Disconnect Claude Code" })).toBeTruthy();
    expect(spoken).toEqual(["Claude Code, Connected"]);
    stop();
  });

  it("shows a failure under its row, and lets the user try again", async () => {
    commands.disconnectAgent.mockResolvedValue(failure("Cursor’s configuration can’t be read."));
    render(<ConnectAgents />);

    const disconnect = await screen.findByRole("button", { name: "Disconnect Cursor" });
    fireEvent.click(disconnect);
    await waitFor(() => expect(group("Cursor").getByRole("alert").textContent).toBe("Cursor’s configuration can’t be read."));
    expect((disconnect as HTMLButtonElement).disabled).toBe(false);
    expect(group("Claude Code").queryByRole("alert")).toBeNull();
  });

  it("adds the command to PATH, then offers to remove it", async () => {
    commands.addCommandToPath.mockResolvedValue(ok(commandLine("added", "Terminals run localhost from ~/.local/bin.")));
    commands.removeCommandFromPath.mockResolvedValue(ok(commandLine("notAdded", "Terminals don’t find localhost.")));
    render(<ConnectAgents />);

    expect(await screen.findByText("Terminals don’t find localhost.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add to PATH" }));
    expect(await screen.findByText("Terminals run localhost from ~/.local/bin.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Remove from PATH" }));
    expect(await screen.findByRole("button", { name: "Add to PATH" })).toBeTruthy();
    expect(commands.removeCommandFromPath).toHaveBeenCalled();
  });

  it.each([
    ["alreadyOnPath", "localhost is already on your PATH."],
    ["taken", "Another file is at ~/.local/bin/localhost."],
  ] as const)("offers nothing when the command is %s", async (state, summary) => {
    commands.commandLineStatus.mockResolvedValue(commandLine(state, summary));
    render(<ConnectAgents />);
    expect(await screen.findByText(summary)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /PATH/ })).toBeNull();
  });

  it("opens the manual setup in the browser", async () => {
    render(<ConnectAgents />);
    fireEvent.click(screen.getByRole("button", { name: "Set up by hand" }));
    expect(commands.openUrl).toHaveBeenCalledWith("https://www.getlocalhost.app/docs/mcp");
    await screen.findByRole("group", { name: "Claude Code" });
  });
});

describe("ConnectAgentsSheet", () => {
  afterEach(() => vi.clearAllMocks());

  it("is a dialog that Done and Escape close", async () => {
    commands.agentStatuses.mockResolvedValue([claude]);
    commands.commandLineStatus.mockResolvedValue(commandLine("notAdded", "Terminals don’t find localhost."));
    const onClose = vi.fn();
    render(<ConnectAgentsSheet onClose={onClose} />);

    const dialog = screen.getByRole("dialog", { name: "Connect your agents" });
    await screen.findByRole("group", { name: "Claude Code" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Done" }));
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
