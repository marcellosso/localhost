import Foundation

// Connecting coding agents (Claude Code, Cursor, Codex) to `localhost mcp`,
// and the bundled `localhost` command on the user's PATH. The core reads and
// edits the agents' files and words every summary; these add Swift labels,
// URLs and typed errors.

extension AgentSetup {
    /// Agents of this user, pointed at `command` (the app's bundled
    /// `localhost`). First-edit backups go in `storageRoot`.
    public convenience init(command: URL, storageRoot: URL = ProjectLibrary.defaultStorageRoot) {
        let setup = AgentSetup.ffiNew(
            command: command.path(percentEncoded: false),
            storageRoot: storageRoot.path(percentEncoded: false)
        )
        self.init(unsafeFromHandle: setup.uniffiCloneHandle())
    }

    /// The command agents are pointed at.
    public var command: URL { URL(corePath: ffiCommand()) }

    /// Every agent localhost knows, installed or not, in a fixed order.
    public func statuses() async -> [AgentStatus] {
        await ffiStatuses()
    }

    /// Adds the `localhost` MCP server to the agent's configuration and
    /// installs the skill, backing the file up the first time.
    @discardableResult
    public func connect(_ client: AgentClient) async throws(LocalhostError) -> AgentStatus {
        try await core { try await ffiConnect(client: client) }
    }

    /// Removes localhost's entry and skill, leaving the rest of the file as it is.
    @discardableResult
    public func disconnect(_ client: AgentClient) async throws(LocalhostError) -> AgentStatus {
        try await core { try await ffiDisconnect(client: client) }
    }

    /// Whether terminals can run `localhost`. Asks the login shell for its
    /// `PATH` the first time, which can take a few seconds.
    public func commandLine() async -> CommandLineStatus {
        await ffiCommandLine()
    }

    @discardableResult
    public func addCommandToPath() async throws(LocalhostError) -> CommandLineStatus {
        try await core { try await ffiAddCommandToPath() }
    }

    @discardableResult
    public func removeCommandFromPath() async throws(LocalhostError) -> CommandLineStatus {
        try await core { try await ffiRemoveCommandFromPath() }
    }
}

extension AgentClient {
    /// "Claude Code", "Cursor", "Codex".
    public var name: String { agentClientName(client: self) }
}

extension AgentStatus: Identifiable {
    public var id: AgentClient { client }
}
