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
                        withAnimation(.easeOut(duration: 0.15)) { showsAppListeners.toggle() }
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
                }
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .columnGlass()
        .overlay {
            if model.listeners.isEmpty {
                ContentUnavailableView("No Listeners", systemImage: "network",
                                       description: Text("Nothing of yours is listening on a TCP port."))
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
        HStack(spacing: 10) {
            Text(listener.ports.map { ":\(String($0))" }.joined(separator: " "))
                .font(.body.monospaced())
                .lineLimit(1)
                .frame(minWidth: 64, alignment: .leading)
            VStack(alignment: .leading, spacing: 2) {
                Text(listener.name)
                    .lineLimit(1)
                Text(listener.command)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Spacer(minLength: 8)
            Text(model.ownerDescription(listener))
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            ListenerStopButton(listener: listener)
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .frame(width: 24)
        }
        .padding(.vertical, 3)
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
            Button(isRun ? "Stop Service" : "Stop…", systemImage: "stop.fill") { model.requestStop(listener) }
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
                        .labelStyle(.titleAndIcon)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: Theme.logBackground))
    }
}
