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
                    .listRowSeparator(.hidden)
                    .tag(AppModel.allServicesID)
            }
            ForEach(Array(project.services.enumerated()), id: \.element.id) { index, service in
                ServiceRow(service: service, status: model.status(of: service), color: Theme.serviceColor(at: index),
                           isSelected: model.selectedServiceID == service.id)
                    .shortcutBadge(index + 1, shownIn: .services)
                    .listRowSeparator(.hidden)
                    .tag(service.id)
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .contextMenu(forSelectionType: UUID.self) { ids in
            if let id = ids.first, id != AppModel.allServicesID, let (_, service) = model.service(id) {
                serviceStateButtons(service)
                Divider()
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
                    Button("Detect Services…") { model.presentedSheet = .detectServices(projectID: project.id) }
                        .buttonStyle(AccentButtonStyle())
                    Button("Add Service…") { model.presentedSheet = .addService(projectID: project.id) }
                        .buttonStyle(PillButtonStyle())
                }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if !project.services.isEmpty { addServiceRow }
        }
        .columnGlass()
        .navigationTitle(project.name)
        .navigationSubtitle((project.path as NSString).abbreviatingWithTildeInPath)
    }

    @ViewBuilder
    private func serviceStateButtons(_ service: Service) -> some View {
        switch model.status(of: service).state {
        case .starting, .running:
            if model.urlToOpen(service) != nil {
                Button("Open in Browser") { model.openURL(of: service.id) }
            }
            Button("Restart") { model.restart(service.id) }
            Button("Stop") { model.stop(service.id) }
        case .stopping:
            EmptyView()
        case .stopped, .exited, .failed:
            Button("Start") { model.start(service.id) }
        }
    }

    private var addServiceRow: some View {
        Button { model.presentedSheet = .addService(projectID: project.id) } label: {
            HStack(spacing: 7) {
                Image(systemName: "plus")
                    .font(.system(size: 11, weight: .semibold))
                    .frame(width: 14)
                Text("Add Service")
                Spacer(minLength: 0)
                KeyHint("⇧⌘N")
            }
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

/// The selected Project's toolbar: Start All, Stop All, its Environment
/// Profile once it has one, and everything else in a `⋯` menu.
struct ProjectToolbar: ToolbarContent {
    let model: AppModel
    let project: Project

    var body: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            if !project.profiles.isEmpty {
                profileMenu
            }
            Button("Start All", systemImage: "play.fill") { model.startAll(inProject: project.id) }
                .buttonStyle(AccentButtonStyle())
                .help("Start every service except Manual ones (⇧⌘R)")
                .disabled(project.services.isEmpty)
            Button("Stop All", systemImage: "stop.fill") { model.stopAll(inProject: project.id) }
                .buttonStyle(PillButtonStyle())
                .help("Stop All (⇧⌘.)")
                .disabled(!project.services.contains { model.status(of: $0).state.isActive })
            MoreMenu(help: "More actions for this project") {
                Button("Edit Project…") { model.presentedSheet = .editProject(project.id) }
                Button("Edit Environment…") { model.presentedSheet = .projectEnvironment(project.id) }
                Button("Detect Services…") { model.presentedSheet = .detectServices(projectID: project.id) }
                Button(project.isFavorite ? "Unfavorite" : "Favorite") { model.setFavorite(project.id, !project.isFavorite) }
                Button("Export Project…") { model.exportProject(project.id) }
                Divider()
                FolderActionButtons(folder: AppModel.folder(of: project))
                Divider()
                Button("Remove Project…", role: .destructive) { model.pendingRemoval = .project(project.id) }
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
            Label(project.activeProfile?.name ?? "No Profile", systemImage: "square.3.layers.3d")
                .labelStyle(.titleAndIcon)
        }
        .fixedSize()
        .help("Environment profile")
    }
}

/// Selects the combined stream of every Service in the Project.
struct AllServicesRow: View {
    @Environment(AppModel.self) private var model
    let project: Project

    var body: some View {
        let states = project.services.map { model.status(of: $0).state }
        let failed = states.filter { $0 == .failed }.count
        HStack(spacing: 9) {
            ServiceDots(services: model.serviceDots(of: project))
            Text("All services")
                .fontWeight(.semibold)
                .lineLimit(1)
            Spacer(minLength: 6)
            Text("\(states.filter(\.isActive).count) of \(states.count)")
                .font(.caption)
                .foregroundStyle(.secondary)
            if failed > 0 {
                Text("\(failed) failed")
                    .font(.caption)
                    .foregroundStyle(Color(nsColor: Theme.failed))
            }
        }
        .padding(.leading, 9)
        .padding(.vertical, 5)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("All services")
        .accessibilityValue("\(states.filter(\.isActive).count) of \(states.count) running" + (failed > 0 ? ", \(failed) failed" : ""))
    }
}

/// A Service: a rail in its colour, its state, name and Port, its command,
/// and, while selected, its actions.
struct ServiceRow: View {
    @Environment(AppModel.self) private var model
    let service: Service
    let status: ServiceStatus
    let color: NSColor
    let isSelected: Bool
    @AppStorage(Preferences.logFontSizeKey) private var logFontSize = Preferences.defaultLogFontSize

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 9) {
                StateIndicator(state: status.state)
                Text(service.name)
                    .fontWeight(.semibold)
                    .lineLimit(1)
                if let port {
                    PortPill(port: port, color: color)
                }
                Spacer(minLength: 6)
                Text(summary)
                    .font(.caption)
                    .foregroundStyle(summaryStyle)
                    .lineLimit(1)
                    .help(summaryHelp)
            }
            Text(service.commandLine)
                .font(.system(size: logFontSize - 1, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.tail)
                .padding(.leading, 23)
            if isSelected {
                ServiceActions(service: service, status: status)
                    .padding(.leading, 23)
                    .padding(.top, 5)
            }
        }
        .padding(.leading, 9)
        .padding(.vertical, 5)
        .overlay(alignment: .leading) {
            Capsule()
                .fill(Color(nsColor: color))
                .frame(width: 3)
                .padding(.vertical, 6)
        }
        .opacity(isManual ? 0.75 : 1)
        .animation(.easeOut(duration: 0.15), value: status.state)
        .animation(.motion(.easeOut(duration: 0.15)), value: isSelected)
        // One element: name, then state, Port and command; the buttons are its actions.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(service.name)
        .accessibilityValue(accessibilityValue)
        .accessibilityActions { ServiceAccessibilityActions(service: service, status: status) }
    }

    /// The state, then the summary when it says more, the Port and the
    /// command, e.g. "Starting, Waiting for API, port 3000, pnpm dev".
    private var accessibilityValue: String {
        let state = StateIndicator.label(status.state)
        let summary = summary.spoken
        var parts = [summary.hasPrefix(state) ? summary : "\(state), \(summary)"]
        if let port, !summary.contains("port \(port)") { parts.append("port \(port)") }
        parts.append(service.commandLine)
        return parts.joined(separator: ", ")
    }

    /// The active Run's port, which may have moved, else the Service's Port.
    private var port: Int? {
        status.state.isActive ? status.run?.port ?? service.port : service.port
    }

    /// Stopped and left out of Start All.
    private var isManual: Bool {
        service.isExcludedFromStartAll && status.state == .stopped
    }

    /// The status summary, naming the Dependencies a Starting Service waits for.
    private var summary: String {
        let awaited = status.awaitedDependencies.compactMap { id in model.service(id)?.service.name }
        if isManual { return "Manual" }
        if status.isRestartNeeded { return "\(status.summary) · restart to apply" }
        guard status.state == .starting, !awaited.isEmpty else { return status.summary }
        return "Waiting for \(ListFormatter.localizedString(byJoining: awaited))…"
    }

    private var summaryHelp: String {
        if isManual { return "Start All skips this service. Start it on its own." }
        if status.isRestartNeeded { return "Its configuration changed since it started. Restart it to apply." }
        return status.portChangeExplanation ?? status.run?.failure?.message ?? status.run?.termination?.summary ?? summary
    }

    private var summaryStyle: AnyShapeStyle {
        if status.state == .failed { return AnyShapeStyle(Color(nsColor: Theme.failed)) }
        if status.hasPortWarning || status.portChangeExplanation != nil || status.isRestartNeeded {
            return AnyShapeStyle(Color(nsColor: Theme.transitioning))
        }
        return AnyShapeStyle(.secondary)
    }
}

/// What a Service row offers VoiceOver, like its buttons: Open, Restart and
/// Stop while live, Start and Edit otherwise.
struct ServiceAccessibilityActions: View {
    @Environment(AppModel.self) private var model
    let service: Service
    let status: ServiceStatus

    var body: some View {
        switch status.state {
        case .starting, .running:
            if model.urlToOpen(service) != nil {
                Button("Open in Browser") { model.openURL(of: service.id) }
            }
            Button("Restart") { model.restart(service.id) }
            Button("Stop") { model.stop(service.id) }
        case .stopping:
            EmptyView()
        case .stopped, .exited, .failed:
            Button("Start") { model.start(service.id) }
        }
        Button("Edit Service") { model.presentedSheet = .editService(service.id) }
    }
}

/// The selected Service's labelled actions, its main one tinted: Open (or
/// Restart) and Stop while live, Start and Edit otherwise.
struct ServiceActions: View {
    @Environment(AppModel.self) private var model
    let service: Service
    let status: ServiceStatus

    var body: some View {
        HStack(spacing: 6) {
            switch status.state {
            case .starting, .running:
                let url = model.urlToOpen(service)
                if let url {
                    Button { model.openURL(of: service.id) } label: {
                        HStack(spacing: 3) {
                            Text("Open")
                            Image(systemName: "arrow.up.right").font(.system(size: 8.5, weight: .bold))
                        }
                    }
                    .buttonStyle(PillButtonStyle(isProminent: true, isCompact: true))
                    .help("Open \(url.absoluteString) (⇧⌘O)")
                    .accessibilityLabel("Open in Browser")
                }
                Button("Restart") { model.restart(service.id) }
                    .buttonStyle(PillButtonStyle(isProminent: url == nil, isCompact: true))
                    .help("Restart (⌘R)")
                Button("Stop") { model.stop(service.id) }
                    .buttonStyle(PillButtonStyle(isCompact: true))
                    .help("Stop (⌘.)")
            case .stopping:
                ProgressView().controlSize(.mini)
                Text("Stopping…").font(.caption).foregroundStyle(.secondary)
            case .stopped, .exited, .failed:
                Button("Start") { model.start(service.id) }
                    .buttonStyle(PillButtonStyle(isProminent: true, isCompact: true))
                    .help("Start (⌘R)")
                Button("Edit…") { model.presentedSheet = .editService(service.id) }
                    .buttonStyle(PillButtonStyle(isCompact: true))
                    .help("Edit Service")
            }
        }
        .frame(height: 22)
    }
}
