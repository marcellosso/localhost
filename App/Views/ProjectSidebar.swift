import LocalhostCore
import SwiftUI

/// The app's name, project search with Add Project, Ports and the Projects,
/// and a footer with Settings and how many Services are running.
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
                    HStack(spacing: 8) {
                        StateIndicator(state: model.aggregateState(of: project))
                        Text(project.name)
                            .lineLimit(1)
                        Spacer(minLength: 0)
                        if project.isFavorite {
                            Image(systemName: "star.fill")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
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
        }
        .listStyle(.sidebar)
        .modifier(SolidSidebar())
        .safeAreaInset(edge: .top, spacing: 0) { SidebarHeader() }
        .safeAreaInset(edge: .bottom, spacing: 0) { SidebarFooter() }
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
            }
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

/// Settings, and how many Services are running across every Project.
private struct SidebarFooter: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        HStack {
            Button("Settings", systemImage: "gearshape") { openWindow(id: "settings") }
                .labelStyle(.iconOnly)
                .buttonStyle(SquareIconButtonStyle())
                .help("Settings (⌘,)")
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

/// The sidebar without its glass, when translucency is off in Settings.
private struct SolidSidebar: ViewModifier {
    @AppStorage(Preferences.translucencyKey) private var isTranslucent = true

    func body(content: Content) -> some View {
        if isTranslucent {
            content
        } else {
            content
                .scrollContentBackground(.hidden)
                .background(Color(nsColor: Theme.sidebarSolid).ignoresSafeArea())
        }
    }
}
