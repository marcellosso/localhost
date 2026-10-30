import Foundation
import LocalhostCore
import Observation
import SwiftUI

/// Coding agents connected to `localhost mcp`, and the `localhost` command
/// in terminals, as Connect Your Agents shows them. The core reads and
/// edits the files; this keeps what it last said, and which action is
/// under way, for the view.
@Observable
final class AgentsModel {
    /// Empty until the first load.
    private(set) var agents: [AgentStatus] = []
    /// `nil` while the login shell is asked for its `PATH`.
    private(set) var commandLine: CommandLineStatus?
    /// The agents whose Connect, Update or Disconnect is under way.
    private(set) var busyAgents: Set<AgentClient> = []
    /// Why an agent's last action failed, in the core's words.
    private(set) var failures: [AgentClient: String] = [:]
    private(set) var isChangingCommandLine = false
    private(set) var commandLineFailure: String?

    @ObservationIgnored private let setup: AgentSetup

    /// The command agents run: the one inside this app, so it always matches
    /// the app's version. A build without it (a debug build that skipped
    /// the helper) still shows the screen; Connect then says it is missing.
    static var bundledCommand: URL {
        Bundle.main.bundleURL.appending(path: "Contents/Helpers/localhost", directoryHint: .notDirectory)
    }

    init(storageRoot: URL) {
        setup = AgentSetup(command: Self.bundledCommand, storageRoot: storageRoot)
    }

    var command: URL { setup.command }

    /// Reads every agent, then the terminal's `PATH`, which can take a few
    /// seconds, so the agents show first.
    func refresh() async {
        agents = await setup.statuses()
        commandLine = await setup.commandLine()
    }

    func connect(_ client: AgentClient) async {
        await change(client) { () throws(LocalhostError) in try await self.setup.connect(client) }
    }

    func disconnect(_ client: AgentClient) async {
        await change(client) { () throws(LocalhostError) in try await self.setup.disconnect(client) }
    }

    func addCommandToPath() async {
        await changeCommandLine { () throws(LocalhostError) in try await self.setup.addCommandToPath() }
    }

    func removeCommandFromPath() async {
        await changeCommandLine { () throws(LocalhostError) in try await self.setup.removeCommandFromPath() }
    }

    private func change(_ client: AgentClient, _ action: () async throws(LocalhostError) -> AgentStatus) async {
        guard !busyAgents.contains(client) else { return }
        busyAgents.insert(client)
        failures[client] = nil
        defer { busyAgents.remove(client) }
        do {
            let status = try await action()
            if let index = agents.firstIndex(where: { $0.client == client }) {
                agents[index] = status
            } else {
                agents = await setup.statuses()
            }
            Self.announce("\(status.name), \(status.connection.label). \(status.summary)", isUrgent: false)
        } catch {
            failures[client] = error.message
            Self.announce("\(client.name): \(error.message)", isUrgent: true)
        }
    }

    private func changeCommandLine(_ action: () async throws(LocalhostError) -> CommandLineStatus) async {
        guard !isChangingCommandLine else { return }
        isChangingCommandLine = true
        commandLineFailure = nil
        defer { isChangingCommandLine = false }
        do {
            let status = try await action()
            commandLine = status
            Self.announce(status.summary, isUrgent: false)
        } catch {
            commandLineFailure = error.message
            Self.announce(error.message, isUrgent: true)
        }
    }

    /// Says how an action ended, since only the row shows it otherwise.
    private static func announce(_ text: String, isUrgent: Bool) {
        var announcement = AttributedString(text.replacingOccurrences(of: "`", with: ""))
        announcement.accessibilitySpeechAnnouncementPriority = isUrgent ? .high : .default
        AccessibilityNotification.Announcement(announcement).post()
    }
}

extension AgentConnection {
    /// The row's short state.
    var label: String {
        switch self {
        case .notInstalled: "Not installed"
        case .notConnected: "Not connected"
        case .connected: "Connected"
        case .needsUpdate: "Update available"
        case .unreadable: "Can’t edit"
        }
    }
}
