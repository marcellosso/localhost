import LocalhostCore
import SwiftUI

/// A Project's Services with their state and Start / Stop / Restart.
struct ProjectDetailView: View {
    @Environment(AppModel.self) private var model
    let project: Project

    var body: some View {
        @Bindable var model = model
        List(selection: $model.selectedServiceID) {
            ForEach(project.services) { service in
                ServiceRow(service: service, status: model.status(of: service))
                    .tag(service.id)
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
        .contextMenu(forSelectionType: UUID.self) { ids in
            if let id = ids.first {
                Button("Edit Service…") { model.presentedSheet = .editService(id) }
                Divider()
                Button("Remove Service…", role: .destructive) { model.pendingRemoval = .service(id) }
            }
        } primaryAction: { ids in
            if let id = ids.first { model.presentedSheet = .editService(id) }
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
        .navigationTitle(project.name)
        .toolbar {
            ToolbarItem {
                Button("Add Service", systemImage: "plus") { model.presentedSheet = .addService(projectID: project.id) }
                    .help("Add Service (⇧⌘N)")
            }
        }
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
            Button("Edit", systemImage: "slider.horizontal.3") { model.presentedSheet = .editProject(project.id) }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .help("Edit Project")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Color(nsColor: .windowBackgroundColor))
        .overlay(alignment: .bottom) { Divider() }
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
                Text(service.commandLine)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Spacer(minLength: 8)
            Text(status.summary)
                .font(.caption)
                .foregroundStyle(status.state == .failed ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary))
                .lineLimit(1)
                .help(status.run?.failure?.message ?? status.run?.termination?.summary ?? status.summary)
            controls
        }
        .padding(.vertical, 3)
        .animation(.easeOut(duration: 0.15), value: status.state)
    }

    @ViewBuilder
    private var controls: some View {
        HStack(spacing: 2) {
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
        .frame(minWidth: 44, alignment: .trailing)
    }
}
