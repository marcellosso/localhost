import LocalhostCore
import SwiftUI

/// A Project's Services with their state and Start / Stop / Restart.
struct ProjectDetailView: View {
    @Environment(AppModel.self) private var model
    let project: Project

    var body: some View {
        @Bindable var model = model
        List(selection: $model.selectedServiceID) {
            if !project.services.isEmpty {
                AllServicesRow(project: project)
                    .tag(AppModel.allServicesID)
            }
            ForEach(project.services) { service in
                ServiceRow(service: service, status: model.status(of: service))
                    .shortcutBadge(project.services.firstIndex { $0.id == service.id }.map { $0 + 1 }, shownIn: .services)
                    .tag(service.id)
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .contextMenu(forSelectionType: UUID.self) { ids in
            if let id = ids.first, id != AppModel.allServicesID {
                Button("Edit Service…") { model.presentedSheet = .editService(id) }
                Divider()
                if let folder = model.folder(ofService: id) { FolderActionButtons(folder: folder) }
                Divider()
                Button("Remove Service…", role: .destructive) { model.pendingRemoval = .service(id) }
            }
        } primaryAction: { ids in
            if let id = ids.first, id != AppModel.allServicesID { model.presentedSheet = .editService(id) }
        }
        .overlay {
            if project.services.isEmpty {
                ContentUnavailableView {
                    Label("No Services", systemImage: "terminal")
                } description: {
                    Text("A service is a command that runs in this project, like “pnpm dev”.")
                } actions: {
                    Button("Add Service…") { model.presentedSheet = .addService(projectID: project.id) }
                }
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            header
        }
        .columnGlass()
        .navigationTitle(project.name)
        .toolbar {
            ToolbarItem {
                Button("Add Service", systemImage: "plus") { model.presentedSheet = .addService(projectID: project.id) }
                    .help("Add Service (⇧⌘N)")
            }
        }
    }

    /// Switches the active Environment Profile; opens the Environment sheet.
    private var profileMenu: some View {
        Menu {
            Picker("Environment Profile", selection: Binding {
                project.activeProfileID
            } set: { model.setActiveProfile($0, inProject: project.id) }) {
                Text("No Profile").tag(UUID?.none)
                ForEach(project.profiles) { profile in
                    Text(profile.name).tag(UUID?.some(profile.id))
                }
            }
            .pickerStyle(.inline)
            Divider()
            Button("Edit Environment…") { model.presentedSheet = .projectEnvironment(project.id) }
        } label: {
            Text(project.activeProfile?.name ?? "No Profile")
                .font(.caption)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .help("Environment profile")
    }

    private var header: some View {
        HStack(spacing: 6) {
            Text(project.key.rawValue)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            Text("·").foregroundStyle(.tertiary)
            Text(project.path)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
            Spacer()
            profileMenu
            Button("Start All", systemImage: "play.fill") { model.startAll(inProject: project.id) }
                .help("Start All (⇧⌘R)")
                .disabled(project.services.isEmpty)
            Button("Stop All", systemImage: "stop.fill") { model.stopAll(inProject: project.id) }
                .help("Stop All (⇧⌘.)")
                .disabled(!project.services.contains { model.status(of: $0).state.isActive })
            Button("Edit", systemImage: "slider.horizontal.3") { model.presentedSheet = .editProject(project.id) }
                .help("Edit Project")
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .overlay(alignment: .bottom) { Divider() }
    }
}

/// Selects the combined stream of every Service in the Project.
struct AllServicesRow: View {
    @Environment(AppModel.self) private var model
    let project: Project

    var body: some View {
        let states = project.services.map { model.status(of: $0).state }
        HStack(spacing: 10) {
            Image(systemName: "square.stack")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 14)
            VStack(alignment: .leading, spacing: 2) {
                Text("All services")
                    .lineLimit(1)
                Text("\(states.filter(\.isActive).count) of \(states.count) running"
                     + (states.contains(.failed) ? " · \(states.filter { $0 == .failed }.count) failed" : ""))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 3)
    }
}

struct ServiceRow: View {
    @Environment(AppModel.self) private var model
    let service: Service
    let status: ServiceStatus

    var body: some View {
        HStack(spacing: 10) {
            StateIndicator(state: status.state)
            VStack(alignment: .leading, spacing: 2) {
                Text(service.name)
                    .lineLimit(1)
                Text(port.map { ":\(String($0)) · \(service.commandLine)" } ?? service.commandLine)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Spacer(minLength: 8)
            Text(summary)
                .font(.caption)
                .foregroundStyle(summaryStyle)
                .lineLimit(1)
                .help(status.isRestartNeeded
                      ? "Its configuration changed since it started. Restart it to apply."
                      : status.portChangeExplanation ?? status.run?.failure?.message ?? status.run?.termination?.summary ?? summary)
            controls
        }
        .padding(.vertical, 3)
        .animation(.easeOut(duration: 0.15), value: status.state)
    }

    /// The active Run's port, which may have moved, else the Service's Port.
    private var port: Int? {
        status.state.isActive ? status.run?.port ?? service.port : service.port
    }

    /// The status summary, naming the Dependencies a Starting Service waits for.
    private var summary: String {
        let awaited = status.awaitedDependencies.compactMap { id in model.service(id)?.service.name }
        if status.isRestartNeeded { return "\(status.summary) · restart to apply" }
        guard status.state == .starting, !awaited.isEmpty else { return status.summary }
        return "Waiting for \(ListFormatter.localizedString(byJoining: awaited))…"
    }

    private var summaryStyle: AnyShapeStyle {
        if status.state == .failed { return AnyShapeStyle(.red) }
        if status.hasPortWarning || status.portChangeExplanation != nil || status.isRestartNeeded { return AnyShapeStyle(.orange) }
        return AnyShapeStyle(.secondary)
    }

    @ViewBuilder
    private var controls: some View {
        HStack(spacing: 2) {
            if status.state == .starting || status.state == .running, let url = model.urlToOpen(service) {
                Button("Open", systemImage: "arrow.up.right.square") { model.openURL(of: service.id) }
                    .help("Open \(url.absoluteString) (⇧⌘O)")
            }
            switch status.state {
            case .starting, .running:
                Button("Restart", systemImage: "arrow.clockwise") { model.restart(service.id) }
                    .help("Restart")
                Button("Stop", systemImage: "stop.fill") { model.stop(service.id) }
                    .help("Stop")
            case .stopping:
                ProgressView().controlSize(.small).frame(width: 40)
            case .stopped, .exited, .failed:
                Button("Start", systemImage: "play.fill") { model.start(service.id) }
                    .help("Start")
            }
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
        .frame(minWidth: 66, alignment: .trailing)
    }
}
