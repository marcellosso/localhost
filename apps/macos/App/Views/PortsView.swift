import LocalhostCore
import SwiftUI

/// Every Listener on this machine, refreshed every 2 seconds while visible:
/// development servers and Docker first, apps and macOS collapsed below.
struct PortsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage("showsAppListeners") private var showsAppListeners = false

    var body: some View {
        @Bindable var model = model
        let development = model.listeners.filter { $0.kind == .development || $0.kind == .docker }
        let others = model.listeners.filter { $0.kind == .app || $0.kind == .system }
        List(selection: $model.selectedListenerID) {
            Section("Development") {
                ForEach(development) { listener in
                    ListenerRow(listener: listener)
                        .tag(listener.id)
                }
                if development.isEmpty, !model.listeners.isEmpty {
                    Text("No development servers are listening.")
                        .foregroundStyle(.secondary)
                }
            }
            if !others.isEmpty {
                // Collapsible sections only work in sidebar lists, so the header toggles them itself.
                Section {
                    if showsAppListeners {
                        ForEach(others) { listener in
                            ListenerRow(listener: listener)
                                .tag(listener.id)
                        }
                    }
                } header: {
                    Button {
                        withAnimation(.motion(.easeOut(duration: 0.15))) { showsAppListeners.toggle() }
                    } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "chevron.right")
                                .rotationEffect(.degrees(showsAppListeners ? 90 : 0))
                                .font(.caption2.weight(.semibold))
                            Text("Apps & macOS (\(others.count))")
                        }
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Apps & macOS, \(others.count) listeners")
                    .accessibilityValue(showsAppListeners ? "Shown" : "Hidden")
                }
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .columnGlass()
        .overlay {
            if model.listeners.isEmpty {
                ContentUnavailableView {
                    Label("No Listeners", systemImage: "network")
                } description: {
                    Text("Nothing of yours is listening on a TCP port. Services you start show up here with their ports, and so does anything else that listens.")
                    KeyTip(before: "Come back with", keys: "⌘0", after: "from anywhere.")
                }
            }
        }
        .navigationTitle("Ports")
        .task {
            while !Task.isCancelled {
                await model.refreshListeners()
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }
}

struct ListenerRow: View {
    @Environment(AppModel.self) private var model
    let listener: Listener

    var body: some View {
        let service = model.service(of: listener)
        HStack(spacing: 10) {
            HStack(spacing: 4) {
                ForEach(listener.ports, id: \.self) { port in
                    if let service {
                        PortPill(port: port, color: model.color(of: service.service))
                    } else {
                        Text(":\(String(port))")
                            .font(.system(size: 11, weight: .medium, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .lineLimit(1)
            .frame(minWidth: 56, alignment: .leading)
            VStack(alignment: .leading, spacing: 2) {
                // A Service's Listener is named by its Service; others by their process.
                if let label = ownerLabel(service) {
                    if service == nil {
                        HStack(spacing: 6) {
                            name
                            label
                        }
                    } else {
                        label
                    }
                } else {
                    name
                }
                Text(Self.subtitle(of: listener))
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Spacer(minLength: 8)
            ListenerStopButton(listener: listener)
                .buttonStyle(PillButtonStyle(isCompact: true))
        }
        .padding(.vertical, 3)
        // One element: whose it is, its ports and command; Stop is its action.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(service.map { "\($0.project.name) › \($0.service.name)" } ?? listener.name)
        .accessibilityValue(accessibilityValue(service))
        .accessibilityActions {
            if model.reasonNotStoppable(listener) == nil, !model.stoppingListenerIDs.contains(listener.id) {
                Button("Stop") { model.requestStop(listener) }
            }
        }
    }

    /// e.g. "port 3000, Python -m http.server 3000, In My SaaS".
    private func accessibilityValue(_ service: (project: Project, service: Service)?) -> String {
        let ports = listener.ports.map(String.init).joined(separator: ", ")
        var parts = [(listener.ports.count == 1 ? "port " : "ports ") + ports, Self.subtitle(of: listener)]
        if service == nil { parts.append(model.ownerDescription(listener)) }
        if model.stoppingListenerIDs.contains(listener.id) { parts.append("stopping") }
        return parts.joined(separator: ", ")
    }

    private var name: some View {
        Text(listener.name)
            .fontWeight(.medium)
            .lineLimit(1)
    }

    /// Whose it is: a Service's coloured "Project › Service", a Project's
    /// folder, or Docker. Unmanaged is the default and shows nothing.
    private func ownerLabel(_ service: (project: Project, service: Service)?) -> AnyView? {
        if let service {
            let color = Color(nsColor: model.color(of: service.service))
            return AnyView(
                Text("\(service.project.name) › \(service.service.name)")
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(color)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(color.opacity(0.12), in: .rect(cornerRadius: 4))
                    .lineLimit(1)
            )
        }
        switch (listener.owner, listener.kind) {
        case (.project, _), (_, .docker):
            return AnyView(Text(model.ownerDescription(listener)).font(.caption).foregroundStyle(.secondary).lineLimit(1))
        default:
            return nil
        }
    }

    /// The command with its executable shortened to a name, e.g.
    /// "Python -m http.server 5199", else the working directory.
    static func subtitle(of listener: Listener) -> String {
        let command = listener.command.trimmingCharacters(in: .whitespaces)
        guard !command.isEmpty else {
            return listener.workingDirectory.map { ($0 as NSString).abbreviatingWithTildeInPath } ?? ""
        }
        if let executable = listener.executablePath, command.hasPrefix(executable) {
            return (executable as NSString).lastPathComponent + command.dropFirst(executable.count)
        }
        let parts = command.split(separator: " ", maxSplits: 1)
        guard let first = parts.first, first.hasPrefix("/") else { return command }
        return (String(first) as NSString).lastPathComponent + (parts.count > 1 ? " " + parts[1] : "")
    }
}

/// Stop for a Listener: its Service for a Run's, otherwise after confirming.
/// Disabled, with the reason, for localhost itself and launchd jobs.
struct ListenerStopButton: View {
    @Environment(AppModel.self) private var model
    let listener: Listener

    var body: some View {
        if model.stoppingListenerIDs.contains(listener.id) {
            ProgressView().controlSize(.small)
        } else {
            let reason = model.reasonNotStoppable(listener)
            Button(isRun ? "Stop" : "Stop…") { model.requestStop(listener) }
                .disabled(reason != nil)
                .help(reason ?? (isRun ? "Stop the service" : "Stop this process"))
        }
    }

    private var isRun: Bool {
        if case .run = listener.owner { true } else { false }
    }
}

/// The selected Listener's process in full.
struct ListenerDetailView: View {
    @Environment(AppModel.self) private var model
    let listener: Listener

    var body: some View {
        Form {
            Section {
                LabeledContent("Ports", value: listener.ports.map(String.init).joined(separator: ", "))
                LabeledContent("Command") {
                    Text(listener.command)
                        .font(.body.monospaced())
                        .textSelection(.enabled)
                        .multilineTextAlignment(.trailing)
                }
                if let path = listener.executablePath {
                    LabeledContent("Executable") {
                        Text(path)
                            .font(.body.monospaced())
                            .textSelection(.enabled)
                            .truncationMode(.middle)
                    }
                }
                LabeledContent("PID", value: String(listener.pid))
                LabeledContent("Process Group", value: String(listener.processGroupID))
                LabeledContent("Working Directory") {
                    Text(listener.workingDirectory.map { ($0 as NSString).abbreviatingWithTildeInPath } ?? "Unknown")
                        .font(.body.monospaced())
                        .textSelection(.enabled)
                        .truncationMode(.middle)
                }
                LabeledContent("Belongs To", value: model.ownerDescription(listener))
                if let label = listener.launchdLabel {
                    LabeledContent("launchd Job", value: label)
                }
            } footer: {
                if listener.isUnmanaged, listener.kind == .development {
                    Text("localhost didn’t start this process and it isn’t inside a project folder.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Section {
                HStack {
                    Spacer()
                    ListenerStopButton(listener: listener)
                        .buttonStyle(PillButtonStyle())
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: Theme.logBackground), ignoresSafeAreaEdges: [])
    }
}
