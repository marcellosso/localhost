import LocalhostCore
import SwiftUI

/// Connect Your Agents as a sheet over the main window: from the app menu,
/// the command palette and the first-run empty state. Settings › Agents
/// shows the same `AgentsContent`.
struct ConnectAgentsView: View {
    static let title = "Connect your agents"
    static let subtitle = "Let Claude Code, Cursor and Codex start, stop and read your Services through localhost. Nothing changes until you click Connect."

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "link")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Theme.accent)
                    .frame(width: 34, height: 34)
                    .background(Theme.accent.opacity(0.15), in: .rect(cornerRadius: 9))
                VStack(alignment: .leading, spacing: 2) {
                    Text(Self.title)
                        .font(.system(size: 16, weight: .semibold))
                        .accessibilityAddTraits(.isHeader)
                    Text(Self.subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 24)
            .padding(.top, 20)
            .padding(.bottom, 14)

            ScrollView {
                AgentsContent()
                    .padding(.horizontal, 24)
                    .padding(.bottom, 16)
            }
            .scrollBounceBehavior(.basedOnSize)

            HStack(spacing: 8) {
                SetUpByHandLink()
                Spacer(minLength: 8)
                Button("Done") { dismiss() }
                    .buttonStyle(AccentButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 14)
            .overlay(alignment: .top) { Divider() }
        }
        .frame(width: 600, height: 560)
        .onExitCommand { dismiss() }
    }
}

/// One row per agent, then the terminal's `localhost`. Loads when it
/// appears; the core words every summary and error.
struct AgentsContent: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let agents = model.agents
        VStack(alignment: .leading, spacing: 0) {
            if agents.agents.isEmpty {
                ProgressView()
                    .controlSize(.small)
                    .frame(maxWidth: .infinity, minHeight: 60)
                    .accessibilityLabel("Looking for agents")
            }
            ForEach(agents.agents) { status in
                AgentRow(status: status)
            }
            Text("USE LOCALHOST IN A TERMINAL")
                .font(.system(size: 11, weight: .semibold))
                .tracking(0.5)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Use localhost in a terminal")
                .accessibilityAddTraits(.isHeader)
                .padding(.top, 22)
                .padding(.bottom, 9)
            CommandLineRow()
        }
        .task { await agents.refresh() }
    }
}

/// An agent: its state and the core's summary, the one action that fits,
/// and what that action writes, folded away.
private struct AgentRow: View {
    @Environment(AppModel.self) private var model
    let status: AgentStatus
    @State private var isShowingChanges = false

    var body: some View {
        let agents = model.agents
        let isBusy = agents.busyAgents.contains(status.client)
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .center, spacing: 16) {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 8) {
                        Text(status.name)
                        stateLabel
                    }
                    Text(coreText: status.summary)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 0)
                if isBusy {
                    ProgressView().controlSize(.small)
                }
                actionButton
                    .disabled(isBusy)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(status.name)
            .accessibilityValue("\(status.connection.label), \(status.summary.replacingOccurrences(of: "`", with: ""))")
            .accessibilityActions {
                if let action, !isBusy {
                    Button(action.title + " " + status.name) { perform(action) }
                }
            }

            if let failure = agents.failures[status.client] {
                Label { Text(coreText: failure) } icon: { Image(systemName: "exclamationmark.triangle.fill") }
                    .font(.caption)
                    .foregroundStyle(Color(nsColor: Theme.failed))
                    .fixedSize(horizontal: false, vertical: true)
            }

            if status.connection != .notInstalled {
                changes
            }
        }
        .padding(.vertical, 11)
        .overlay(alignment: .top) { Divider() }
    }

    @ViewBuilder
    private var stateLabel: some View {
        switch status.connection {
        case .connected:
            Label(status.connection.label, systemImage: "checkmark.circle.fill")
                .labelStyle(StateLabelStyle())
                .foregroundStyle(Theme.accentText)
        case .needsUpdate:
            Label(status.connection.label, systemImage: "arrow.triangle.2.circlepath")
                .labelStyle(StateLabelStyle())
                .foregroundStyle(Theme.accentText)
        case .notConnected, .notInstalled, .unreadable:
            Text(status.connection.label)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: Action

    private enum Action {
        case connect, update, disconnect

        var title: String {
            switch self {
            case .connect: "Connect"
            case .update: "Update"
            case .disconnect: "Disconnect"
            }
        }
    }

    private var action: Action? {
        switch status.connection {
        case .notConnected: .connect
        case .needsUpdate: .update
        case .connected: .disconnect
        case .notInstalled, .unreadable: nil
        }
    }

    @ViewBuilder
    private var actionButton: some View {
        if let action {
            let button = Button(action.title) { perform(action) }
                .help("\(action.title) \(status.name)")
            if action == .disconnect {
                button.buttonStyle(PillButtonStyle(isCompact: true))
            } else {
                button.buttonStyle(AccentButtonStyle(isCompact: true))
            }
        }
    }

    private func perform(_ action: Action) {
        let agents = model.agents
        let client = status.client
        Task {
            switch action {
            case .connect, .update: await agents.connect(client)
            case .disconnect: await agents.disconnect(client)
            }
        }
    }

    // MARK: What changes

    private var changes: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                withAnimation(.motion(.snappy(duration: 0.2))) { isShowingChanges.toggle() }
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .rotationEffect(.degrees(isShowingChanges ? 90 : 0))
                    Text("What changes")
                }
                .font(.caption)
                .foregroundStyle(Theme.accentText)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("What changes for \(status.name)")
            .accessibilityValue(isShowingChanges ? "Shown" : "Hidden")

            if isShowingChanges {
                VStack(alignment: .leading, spacing: 6) {
                    pathLine("Adds to", status.configPath, after: ":")
                    Text(status.entry)
                        .font(.system(size: 11, design: .monospaced))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.primary.opacity(0.05), in: .rect(cornerRadius: 6))
                        .overlay { RoundedRectangle(cornerRadius: 6).strokeBorder(Color.primary.opacity(0.08)) }
                    pathLine("Installs the skill at", status.skillPath)
                    if let configured = status.configuredCommand, configured != model.agents.command.path(percentEncoded: false) {
                        pathLine("Now runs", configured)
                    }
                    if let backup = status.backupPath {
                        pathLine("Original saved at", backup)
                    }
                }
                .font(.caption)
                .padding(.leading, 13)
            }
        }
    }

    /// "Adds to ~/.claude.json:", the path monospaced and selectable.
    private func pathLine(_ before: String, _ path: String, after: String = "") -> some View {
        let path = (path as NSString).abbreviatingWithTildeInPath
        return Text("\(before) \(Text(path).monospaced())\(after)")
            .foregroundStyle(.secondary)
            .textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
    }
}

private struct StateLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 3) {
            configuration.icon
            configuration.title
        }
        .font(.caption.weight(.medium))
    }
}

/// The bundled `localhost` on the user's `PATH`, so terminals can run it too.
private struct CommandLineRow: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let agents = model.agents
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .center, spacing: 16) {
                if let status = agents.commandLine {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(coreText: status.summary)
                            .fixedSize(horizontal: false, vertical: true)
                        Text((status.path as NSString).abbreviatingWithTildeInPath)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                    .accessibilityElement(children: .combine)
                    Spacer(minLength: 0)
                    if agents.isChangingCommandLine {
                        ProgressView().controlSize(.small)
                    }
                    switch status.state {
                    case .notAdded:
                        Button("Add to PATH") { Task { await agents.addCommandToPath() } }
                            .buttonStyle(PillButtonStyle(isProminent: true, isCompact: true))
                            .disabled(agents.isChangingCommandLine)
                    case .added:
                        Button("Remove from PATH") { Task { await agents.removeCommandFromPath() } }
                            .buttonStyle(PillButtonStyle(isCompact: true))
                            .disabled(agents.isChangingCommandLine)
                    case .alreadyOnPath, .taken:
                        EmptyView()
                    }
                } else {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text("Checking your PATH…")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .accessibilityElement(children: .combine)
                    Spacer(minLength: 0)
                }
            }
            if let failure = agents.commandLineFailure {
                Label { Text(coreText: failure) } icon: { Image(systemName: "exclamationmark.triangle.fill") }
                    .font(.caption)
                    .foregroundStyle(Color(nsColor: Theme.failed))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, 11)
        .overlay(alignment: .top) { Divider() }
    }
}

/// The docs page for setting up an agent without the app's help.
struct SetUpByHandLink: View {
    static let url = URL(string: "https://www.getlocalhost.app/docs/mcp")!

    var body: some View {
        Link("Set up by hand", destination: Self.url)
            .font(.caption)
            .foregroundStyle(Theme.accentText)
            .pointerStyle(.link)
            .help(Self.url.absoluteString)
    }
}

extension Text {
    /// Text the core wrote, its `code` spans in monospace without the backticks.
    init(coreText: String) {
        var text = AttributedString()
        for (index, part) in coreText.split(separator: "`", omittingEmptySubsequences: false).enumerated() {
            var piece = AttributedString(String(part))
            if !index.isMultiple(of: 2) { piece.inlinePresentationIntent = .code }
            text += piece
        }
        self.init(text)
    }
}
