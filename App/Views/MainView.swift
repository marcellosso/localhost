import LocalhostCore
import SwiftUI

/// Sidebar of Projects → the selected Project's Services → the selected Service's log.
struct MainView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            ProjectSidebar()
                .navigationSplitViewColumnWidth(min: 180, ideal: 208, max: 320)
        } content: {
            Group {
                if model.sidebarSelection == .ports {
                    PortsView()
                } else if let project = model.selectedProject {
                    ProjectDetailView(project: project)
                } else {
                    ContentUnavailableView {
                        Label(model.projects.isEmpty ? "No Projects" : "No Project Selected", systemImage: "folder")
                    } description: {
                        Text(model.projects.isEmpty ? "Add a project folder to get started." : "Select a project in the sidebar.")
                    } actions: {
                        if model.projects.isEmpty, model.core != nil {
                            Button("Add Project…") { model.presentedSheet = .addProject }
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .columnGlass()
                }
            }
            .navigationSplitViewColumnWidth(min: 240, ideal: 280, max: 380)
        } detail: {
            Group {
                if model.sidebarSelection == .ports {
                    if let listener = model.selectedListenerID.flatMap(model.listener) {
                        ListenerDetailView(listener: listener)
                    } else {
                        ContentUnavailableView("No Listener Selected", systemImage: "network",
                                               description: Text("Select a listener to see its process."))
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                            .background(Color(nsColor: Theme.logBackground), ignoresSafeAreaEdges: [])
                    }
                } else if model.isAllServicesSelected, let project = model.selectedProject {
                    CombinedLogPane(project: project)
                } else if let service = model.selectedService {
                    LogPane(service: service)
                } else {
                    ContentUnavailableView("No Service Selected", systemImage: "text.alignleft",
                                           description: Text("Select a service to see its output."))
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(Color(nsColor: Theme.logBackground), ignoresSafeAreaEdges: [])
                }
            }
            // The toolbar above the log is glass like the services column; the log stays opaque.
            .columnGlass()
            .toolbar {
                if model.sidebarSelection != .ports, let project = model.selectedProject {
                    ProjectToolbar(model: model, project: project)
                }
            }
        }
        .toolbarBackground(.hidden, for: .windowToolbar)
        .background(ToolbarStripSplitView())
        .sheet(item: $model.presentedSheet) { sheet in
            switch sheet {
            case .addProject:
                ProjectForm(editing: nil)
            case .editProject(let id):
                ProjectForm(editing: model.project(id))
            case .addService(let projectID):
                if let project = model.project(projectID) { ServiceForm(project: project, editing: nil) }
            case .editService(let id):
                if let (project, service) = model.service(id) { ServiceForm(project: project, editing: service) }
            case .projectEnvironment(let id):
                if let project = model.project(id) { EnvironmentForm(project: project) }
            }
        }
        .confirmationDialog(removalTitle, isPresented: isConfirmingRemoval, presenting: model.pendingRemoval) { removal in
            Button("Remove", role: .destructive) {
                Task {
                    switch removal {
                    case .project(let id): await model.removeProject(id)
                    case .service(let id): await model.removeService(id)
                    }
                }
            }
        } message: { removal in
            Text(removalMessage(removal))
        }
        .confirmationDialog(
            listenerStopTitle, isPresented: isConfirmingListenerStop, presenting: model.pendingListenerStop
        ) { listener in
            Button("Stop", role: .destructive) { Task { await model.stopListener(listener) } }
        } message: { listener in
            Text(Self.processDescription(listener))
        }
        .alert(blockedStartTitle, isPresented: isShowingBlockedStart, presenting: model.blockedStart) { blocked in
            if let holder = blocked.holder, model.reasonNotStoppable(holder) == nil {
                Button("Stop It and Start") { Task { await model.stopHolderAndStart(blocked) } }
            }
            if let port = blocked.nextFreePort {
                Button("Start on \(String(port))") { model.start(blocked.serviceID, onPort: port) }
            }
            Button("Cancel", role: .cancel) {}
        } message: { blocked in
            Text(blockedStartMessage(blocked))
        }
        .alert(model.orphanAlertTitle, isPresented: $model.isShowingOrphanAlert) {
            Button("Stop Them") { Task { await model.stopOrphans() } }
                .keyboardShortcut(.defaultAction)
            Button("Leave Them", role: .cancel) { model.leaveOrphans() }
        } message: {
            Text(model.orphanDescriptions.joined(separator: "\n"))
        }
        .alert("Something went wrong", isPresented: isShowingActionError, presenting: model.actionError) { _ in
            Button("OK") {}
        } message: { error in
            Text(error.message)
        }
        .shortcutBadgeHost()
        .overlay(alignment: .bottom) {
            if model.isStoppingOrphans {
                ProgressView("Stopping services from the last session…")
                    .controlSize(.small)
                    .padding(8)
                    .background(.regularMaterial, in: .rect(cornerRadius: 8))
                    .padding()
            } else if let error = model.startupError {
                Text("Could not load projects: \(error.message)")
                    .foregroundStyle(.red)
                    .padding()
            }
        }
    }

    private var removalTitle: String {
        switch model.pendingRemoval {
        case .project(let id): "Remove “\(model.project(id)?.name ?? "project")”?"
        case .service(let id): "Remove “\(model.service(id)?.service.name ?? "service")”?"
        case nil: ""
        }
    }

    private func removalMessage(_ removal: AppModel.Removal) -> String {
        var message = "Running services are stopped first. Their configuration is deleted; the project folder is not touched."
        if case .service(let id) = removal, let (project, _) = model.service(id) {
            let dependents = project.dependents(of: id).map { "“\($0.name)”" }
            if !dependents.isEmpty {
                message += "\n\n\(ListFormatter.localizedString(byJoining: dependents)) will no longer depend on it."
            }
        }
        return message
    }

    private var listenerStopTitle: String {
        guard let listener = model.pendingListenerStop else { return "" }
        return "Stop \(listener.name) on port\(listener.ports.count == 1 ? "" : "s") \(listener.ports.map(String.init).joined(separator: ", "))?"
    }

    /// e.g. "python3 -m http.server 3000\nPID 123 · ~/code/site"
    static func processDescription(_ listener: Listener) -> String {
        let directory = listener.workingDirectory.map { ($0 as NSString).abbreviatingWithTildeInPath }
        return "\(listener.command)\nPID \(listener.pid)\(directory.map { " · \($0)" } ?? "")"
    }

    private var blockedStartTitle: String {
        guard let blocked = model.blockedStart else { return "" }
        let name = model.service(blocked.serviceID)?.service.name ?? "The service"
        return "\(name) can’t start: port \(String(blocked.port)) is in use"
    }

    private func blockedStartMessage(_ blocked: AppModel.BlockedStart) -> String {
        var message: String
        if let holder = blocked.holder {
            message = "It is held by \(Self.processDescription(holder))\n(\(model.ownerDescription(holder)))."
            if let reason = model.reasonNotStoppable(holder) { message += "\n\n\(reason)" }
        } else {
            message = "Another process, which localhost can’t inspect, is listening on it."
        }
        if blocked.nextFreePort == nil {
            message += "\n\nNo free port was found within 100 ports above it."
        }
        if let (project, _) = model.service(blocked.serviceID) {
            let dependents = project.dependents(of: blocked.serviceID).map { "“\($0.name)”" }
            if !dependents.isEmpty, let port = blocked.nextFreePort {
                message += "\n\nStarting on \(String(port)): \(ListFormatter.localizedString(byJoining: dependents)) depend\(dependents.count == 1 ? "s" : "") on it and may still expect port \(String(blocked.port))."
            }
        }
        return message
    }

    private var isConfirmingListenerStop: Binding<Bool> {
        Binding { model.pendingListenerStop != nil } set: { if !$0 { model.pendingListenerStop = nil } }
    }

    private var isShowingBlockedStart: Binding<Bool> {
        Binding { model.blockedStart != nil } set: { if !$0 { model.blockedStart = nil } }
    }

    private var isConfirmingRemoval: Binding<Bool> {
        Binding { model.pendingRemoval != nil } set: { if !$0 { model.pendingRemoval = nil } }
    }

    private var isShowingActionError: Binding<Bool> {
        Binding { model.actionError != nil } set: { if !$0 { model.actionError = nil } }
    }
}
