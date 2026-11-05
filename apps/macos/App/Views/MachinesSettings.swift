import LocalhostCore
import SwiftUI

/// Settings › Machines: the other computers whose Projects this Mac shows,
/// each with where `ssh` reaches it and how it is, and Add Machine.
struct MachinesSettings: View {
    @Environment(AppModel.self) private var model
    @State private var isAdding = false
    @State private var renaming: Machine?
    @State private var newName = ""
    @State private var removing: Machine?

    static let title = "Machines"
    static let subtitle = "Other computers whose projects you run from here, over SSH. Their projects, secrets and services stay on them."

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SectionHeader(title: Self.title, subtitle: Self.subtitle)
            if let error = model.machinesError {
                Label(error.message, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(Color(nsColor: Theme.failed))
                    .padding(.bottom, 12)
            }
            ForEach(model.shownMachines) { shown in
                MachineSettingsRow(shown: shown) {
                    newName = shown.machine.name
                    renaming = shown.machine
                } remove: {
                    removing = shown.machine
                }
            }
            if model.shownMachines.isEmpty, model.machinesError == nil {
                Text("No machines yet. Add one you can reach with ssh, like a dev box on your network or over Tailscale.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.vertical, 11)
                    .overlay(alignment: .top) { Divider() }
            }
            HStack {
                Spacer()
                Button("Add Machine…", systemImage: "plus") { isAdding = true }
                    .buttonStyle(PillButtonStyle())
                    .disabled(model.machines == nil)
            }
            .padding(.top, 12)
            .overlay(alignment: .top) { Divider() }
        }
        .sheet(isPresented: $isAdding) {
            AddMachineForm()
                .environment(model)
        }
        .alert("Rename Machine", isPresented: Binding { renaming != nil } set: { if !$0 { renaming = nil } },
               presenting: renaming) { machine in
            TextField("Name", text: $newName)
            Button("Rename") { model.renameMachine(machine.id, to: newName) }
            Button("Cancel", role: .cancel) {}
        } message: { machine in
            Text("What localhost calls \(machine.destination). Leave it empty to use that.")
        }
        .confirmationDialog(
            "Remove “\(removing?.name ?? "machine")”?",
            isPresented: Binding { removing != nil } set: { if !$0 { removing = nil } },
            presenting: removing
        ) { machine in
            Button("Remove", role: .destructive) { model.removeMachine(machine.id) }
        } message: { machine in
            Text("localhost stops showing it here and closes its forwarded ports. Nothing changes on \(machine.name): its projects and services stay as they are.")
        }
    }
}

/// A Machine: its name, the destination `ssh` reaches it at, how it is, and
/// a `⋯` menu with Reconnect, Rename and Remove.
private struct MachineSettingsRow: View {
    @Environment(AppModel.self) private var model
    let shown: MachinesStore.Shown
    let rename: () -> Void
    let remove: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(shown.machine.name)
                    if shown.machine.name != shown.machine.destination {
                        Text(shown.machine.destination)
                            .font(.callout.monospaced())
                            .foregroundStyle(.secondary)
                    }
                }
                HStack(alignment: .firstTextBaseline, spacing: 5) {
                    if case .connecting = shown.state {
                        ProgressView().controlSize(.mini)
                    } else if case .unavailable = shown.state {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .accessibilityHidden(true)
                    }
                    Text(model.stateDescription(of: shown))
                        .fixedSize(horizontal: false, vertical: true)
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if model.needsReconnect(shown) {
                Button("Reconnect") { model.reconnectMachine(shown.id) }
                    .buttonStyle(PillButtonStyle(isCompact: true))
                    .accessibilityLabel("Reconnect \(shown.machine.name)")
            }
            MoreMenu(help: "More actions for \(shown.machine.name)") {
                Button("Reconnect") { model.reconnectMachine(shown.id) }
                Button("Rename…", action: rename)
                Divider()
                Button("Remove…", role: .destructive, action: remove)
            }
            .accessibilityLabel("More actions for \(shown.machine.name)")
        }
        .padding(.vertical, 11)
        .overlay(alignment: .top) { Divider() }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(shown.machine.name)
    }
}

/// Add Machine: where `ssh` reaches it (a Host of `~/.ssh/config`, offered
/// from a menu, or `user@host:port`) and what to call it. Add connects
/// first, and only once that works is the Machine kept; otherwise it says
/// why, and what to do next.
struct AddMachineForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var destination = ""
    @State private var name = ""
    @State private var hosts: [String] = []
    @State private var problem: String?
    @State private var isConnecting = false

    private var trimmedDestination: String {
        destination.trimmingCharacters(in: .whitespaces)
    }

    var body: some View {
        FormSheet(
            systemImage: "desktopcomputer",
            title: "Add Machine",
            subtitle: Text("Another computer you reach with ssh."),
            hint: isConnecting ? "Connecting to \(trimmedDestination)…" : "↩ to connect and add",
            primaryTitle: isConnecting ? "Connecting…" : "Add",
            isPrimaryDisabled: trimmedDestination.isEmpty || isConnecting,
            width: 480,
            cancel: { dismiss() },
            primary: { Task { await add() } }
        ) {
            Field(title: "SSH host",
                  hint: "A host from ~/.ssh/config, or user@host:port. localhost connects as ssh does, with your keys and settings.") {
                HStack(spacing: 6) {
                    BoxedTextField(label: "SSH host", text: $destination, prompt: hosts.first ?? "devbox", isMonospaced: true)
                    if !hosts.isEmpty {
                        Menu {
                            ForEach(hosts, id: \.self) { host in
                                Button(host) { destination = host }
                            }
                        } label: {
                            Label("Hosts in ~/.ssh/config", systemImage: "chevron.down")
                                .labelStyle(.iconOnly)
                        }
                        .menuStyle(.borderlessButton)
                        .menuIndicator(.hidden)
                        .fixedSize()
                        .help("Hosts in ~/.ssh/config")
                    }
                }
            }
            Field(title: "Name", note: "optional") {
                BoxedTextField(label: "Name", text: $name, prompt: trimmedDestination.isEmpty ? "Dev box" : trimmedDestination)
            }
            // Often longer than the footer holds, and says what to type in a terminal.
            if let problem {
                Label(problem, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(Color(nsColor: Theme.failed))
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
        }
        .disabled(isConnecting)
        .onChange(of: destination) { problem = nil }
        .task { hosts = await Machines.sshConfigHosts() }
    }

    private func add() async {
        isConnecting = true
        defer { isConnecting = false }
        do {
            try await model.addMachine(destination: trimmedDestination, name: name)
            dismiss()
        } catch {
            problem = error.message
        }
    }
}
