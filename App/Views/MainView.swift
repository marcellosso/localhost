import LocalhostCore
import SwiftUI

/// Sidebar of Projects → the selected Project's Services → the selected Service's log.
struct MainView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            ProjectSidebar()
                .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 320)
        } content: {
            Group {
                if let project = model.selectedProject {
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
                }
            }
            .navigationSplitViewColumnWidth(min: 320, ideal: 400)
        } detail: {
            if let service = model.selectedService {
                LogPane(service: service)
            } else {
                ContentUnavailableView("No Service Selected", systemImage: "text.alignleft",
                                       description: Text("Select a service to see its output."))
                    .background(Color(nsColor: .textBackgroundColor))
            }
        }
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
        } message: { _ in
            Text("Running services are stopped first. Their configuration is deleted; the project folder is not touched.")
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

    private var isConfirmingRemoval: Binding<Bool> {
        Binding { model.pendingRemoval != nil } set: { if !$0 { model.pendingRemoval = nil } }
    }

    private var isShowingActionError: Binding<Bool> {
        Binding { model.actionError != nil } set: { if !$0 { model.actionError = nil } }
    }
}
