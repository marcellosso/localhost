import LocalhostCore
import SwiftUI

/// The app's name, project search with Add Project, Ports and the Projects,
/// then each other Machine's, a newer version when there is one, and a
/// footer with Settings, Keyboard Shortcuts and how many Services are running.
struct ProjectSidebar: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        let projects = model.filteredProjects
        List(selection: $model.sidebarSelection) {
            Label("Ports", systemImage: "network")
                .tag(AppModel.SidebarItem.ports)
            Section("Projects") {
                ForEach(projects) { project in
                    ProjectRow(project: project, place: .thisMac)
                        .shortcutBadge(model.projects.firstIndex { $0.id == project.id }.map { $0 + 1 }, shownIn: .projects)
                        .tag(AppModel.SidebarItem.project(project.id))
                }
                if projects.isEmpty, !model.projectSearch.isEmpty {
                    Text("No projects match “\(model.projectSearch)”.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .selectionDisabled()
                }
            }
            ForEach(model.shownMachines) { shown in
                Section {
                    MachineStateRow(shown: shown)
                    ForEach(model.filtered(shown.projects)) { project in
                        ProjectRow(project: project, place: .machine(shown.id))
                            .opacity(shown.isConnected ? 1 : 0.6)
                            .tag(AppModel.SidebarItem.machineProject(project.id, machine: shown.id))
                    }
                } header: {
                    Text(shown.machine.name)
                        .accessibilityLabel("Machine \(shown.machine.name)")
                }
            }
        }
        .listStyle(.sidebar)
        .modifier(SolidSidebar())
        .safeAreaInset(edge: .top, spacing: 0) { SidebarHeader() }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            VStack(spacing: 0) {
                TelemetryNotice()
                UpdateNotice()
                SidebarFooter()
            }
        }
        .contextMenu(forSelectionType: AppModel.SidebarItem.self) { items in
            if case .project(let id) = items.first, let project = model.project(id) {
                Button("Edit Project…") { model.presentedSheet = .editProject(id) }
                Button("Edit Environment…") { model.presentedSheet = .projectEnvironment(id) }
                Button(project.isFavorite ? "Unfavorite" : "Favorite") { model.setFavorite(id, !project.isFavorite) }
                Button("Export Project…") { model.exportProject(id) }
                Divider()
                FolderActionButtons(folder: AppModel.folder(of: project))
                Divider()
                Button("Remove Project…", role: .destructive) { model.pendingRemoval = .project(id) }
            } else if case .machineProject(let id, let machineID) = items.first,
                      let project = model.project(id, on: .machine(machineID)) {
                let place = AppModel.Place.machine(machineID)
                Button("Start All") { model.startAll(inProject: id, on: place) }
                    .disabled(project.services.isEmpty || !model.isReachable(place))
                Button("Stop All") { model.stopAll(inProject: id, on: place) }
                    .disabled(!project.services.contains { model.status(of: $0, on: place).state.isActive } || !model.isReachable(place))
                Divider()
                Button("Reconnect") { model.reconnectMachine(machineID) }
            }
        }
    }
}

/// A Project in the sidebar, on this Mac or another Machine: the state of
/// its Services, its name, and whether it is a favourite.
private struct ProjectRow: View {
    @Environment(AppModel.self) private var model
    let project: Project
    let place: AppModel.Place

    var body: some View {
        HStack(spacing: 8) {
            StateIndicator(state: model.aggregateState(of: project, on: place))
            Text(project.name)
                .lineLimit(1)
            Spacer(minLength: 0)
            if project.isFavorite {
                Image(systemName: "star.fill")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(project.name + (project.isFavorite ? ", favourite" : ""))
        .accessibilityValue(model.accessibilityState(of: project, on: place))
    }
}

/// How another Machine is, under its name, while that isn't plainly
/// connected: connecting, why it can't be reached (with Reconnect when it
/// waits for the user), or that it has no Projects.
private struct MachineStateRow: View {
    @Environment(AppModel.self) private var model
    let shown: MachinesStore.Shown

    /// Nothing while connected with Projects, or reconnecting to them.
    private var text: String? {
        switch shown.state {
        case .connected:
            return shown.projects.isEmpty ? "No projects on \(shown.machine.name)." : nil
        case .connecting where !shown.projects.isEmpty:
            return nil
        default:
            return model.stateDescription(of: shown)
        }
    }

    var body: some View {
        if let text {
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    if case .connecting = shown.state {
                        ProgressView().controlSize(.mini)
                    } else if case .unavailable = shown.state {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.secondary)
                            .accessibilityHidden(true)
                    }
                    Text(text)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if model.needsReconnect(shown) {
                    Button("Reconnect") { model.reconnectMachine(shown.id) }
                        .buttonStyle(PillButtonStyle(isCompact: true))
                        .accessibilityLabel("Reconnect \(shown.machine.name)")
                }
            }
            .padding(.vertical, 2)
            .selectionDisabled()
        }
    }
}

/// The `localhost:` wordmark, then project search with Add Project beside it.
private struct SidebarHeader: View {
    @Environment(AppModel.self) private var model
    @FocusState private var isSearchFocused: Bool

    var body: some View {
        @Bindable var model = model
        VStack(alignment: .leading, spacing: 12) {
            (Text("localhost") + Text(":").foregroundStyle(Theme.accent))
                .font(.system(size: 17, weight: .semibold, design: .monospaced))
                .tracking(-0.3)
                .padding(.leading, 6)
                .accessibilityLabel("localhost")
            HStack(spacing: 6) {
                HStack(spacing: 6) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                    TextField("Search projects", text: $model.projectSearch)
                        .textFieldStyle(.plain)
                        .focused($isSearchFocused)
                        .onExitCommand { model.projectSearch = "" }
                        // ↩ opens the first match's Services, ↓ moves into the list.
                        .onSubmit {
                            guard let project = model.filteredProjects.first else { return }
                            model.selectedProjectID = project.id
                            model.focusColumn(.list)
                        }
                        .onKeyPress(.downArrow) {
                            model.focusColumn(.sidebar)
                            return .handled
                        }
                    if !model.projectSearch.isEmpty {
                        Button("Clear Search", systemImage: "xmark.circle.fill") { model.projectSearch = "" }
                            .labelStyle(.iconOnly)
                            .buttonStyle(.plain)
                            .foregroundStyle(.secondary)
                    }
                }
                .font(.callout)
                .padding(.horizontal, 8)
                .frame(height: 28)
                .background(Color.primary.opacity(isSearchFocused ? 0.09 : 0.06), in: .rect(cornerRadius: 7))
                Button("Add Project", systemImage: "plus") { model.presentedSheet = .addProject }
                    .labelStyle(.iconOnly)
                    .buttonStyle(SquareIconButtonStyle())
                    .help("Add Project (⌘N)")
                    .disabled(model.core == nil)
            }
        }
        .padding(.horizontal, 10)
        .padding(.top, 2)
        .padding(.bottom, 8)
    }
}

/// Settings and Keyboard Shortcuts, and how many Services are running across every Project.
private struct SidebarFooter: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        HStack {
            Button("Settings", systemImage: "gearshape") { openWindow(id: "settings") }
                .labelStyle(.iconOnly)
                .buttonStyle(SquareIconButtonStyle())
                .help("Settings (⌘,)")
            Button("Keyboard Shortcuts", systemImage: "keyboard") { model.toggleKeyboardShortcuts() }
                .labelStyle(.iconOnly)
                .buttonStyle(SquareIconButtonStyle())
                .help("Keyboard Shortcuts (⌘/)")
            Spacer()
            let running = model.activeServiceCount
            if running > 0 {
                Text("\(running) running")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .overlay(alignment: .top) { Divider() }
    }
}

/// A 28pt square icon button that fills on hover.
struct SquareIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        HoverSquare(configuration: configuration)
    }

    private struct HoverSquare: View {
        let configuration: Configuration
        @Environment(\.isEnabled) private var isEnabled
        @State private var isHovered = false

        var body: some View {
            configuration.label
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.secondary)
                .frame(width: 28, height: 28)
                .background(Color.primary.opacity(configuration.isPressed ? 0.12 : isHovered ? 0.07 : 0), in: .rect(cornerRadius: 7))
                .contentShape(.rect)
                .opacity(isEnabled ? 1 : 0.4)
                .onHover { isHovered = $0 }
        }
    }
}

/// The sidebar without its glass, when translucency is off in Settings or
/// under Reduce Transparency.
private struct SolidSidebar: ViewModifier {
    @AppStorage(Preferences.translucencyKey) private var isTranslucent = true
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    func body(content: Content) -> some View {
        if isTranslucent && !reduceTransparency {
            content
        } else {
            content
                .scrollContentBackground(.hidden)
                .background(Color(nsColor: Theme.sidebarSolid).ignoresSafeArea())
        }
    }
}
