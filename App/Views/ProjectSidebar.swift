import LocalhostCore
import SwiftUI

struct ProjectSidebar: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        List(selection: $model.sidebarSelection) {
            Label("Ports", systemImage: "network")
                .tag(AppModel.SidebarItem.ports)
            Section("Projects") {
                ForEach(model.projects) { project in
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
                    .tag(AppModel.SidebarItem.project(project.id))
                }
            }
        }
        .listStyle(.sidebar)
        .contextMenu(forSelectionType: AppModel.SidebarItem.self) { items in
            if case .project(let id) = items.first, let project = model.project(id) {
                Button("Edit Project…") { model.presentedSheet = .editProject(id) }
                Button("Edit Environment…") { model.presentedSheet = .projectEnvironment(id) }
                Button(project.isFavorite ? "Unfavorite" : "Favorite") { model.setFavorite(id, !project.isFavorite) }
                Divider()
                Button("Remove Project…", role: .destructive) { model.pendingRemoval = .project(id) }
            }
        }
        .toolbar {
            ToolbarItem {
                Button("Add Project", systemImage: "plus") { model.presentedSheet = .addProject }
                    .help("Add Project (⌘N)")
                    .disabled(model.core == nil)
            }
        }
    }
}
