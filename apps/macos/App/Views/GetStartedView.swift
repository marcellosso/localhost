import LocalhostCore
import SwiftUI

/// The Get started checklist, at the foot of the sidebar once there is a
/// Project: each row ticks from what is true, an open row takes the user
/// there. It goes away for good when every row is ticked or it is closed.
/// Ticks use the accent: green belongs to Service State.
struct GetStartedView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if let onboarding = model.onboarding, onboarding.isChecklistShown(projectCount: model.projects.count) {
            let isRunning = model.hasRunningService
            let items = onboarding.checklistItems(
                projects: model.projects, hasRunningService: isRunning, hasConnectedAgent: model.agents.hasConnectedAgent
            )
            let isComplete = items.allSatisfy(\.isDone)
            Group {
                if !isComplete {
                    checklist(items)
                }
            }
            // Agents are read again as each sheet closes, Connect Your Agents among them.
            .task(id: model.presentedSheet == nil) {
                if model.presentedSheet == nil { await model.agents.refreshAgents() }
            }
            .onChange(of: isRunning, initial: true) {
                if isRunning { model.updateOnboarding { $0.hasStartedService = true } }
            }
            .onChange(of: isComplete, initial: true) {
                if isComplete { model.updateOnboarding { $0.isChecklistDone = true } }
            }
        }
    }

    private func checklist(_ items: [Onboarding.ChecklistItem]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Text(Onboarding.checklistTitle)
                    .fontWeight(.semibold)
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 0)
                Text("\(items.filter(\.isDone).count) of \(items.count)")
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .foregroundStyle(.secondary)
                Button("Hide Get Started", systemImage: "xmark") {
                    model.updateOnboarding { $0.isChecklistDone = true }
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.secondary)
                .help("Hide Get Started")
            }
            VStack(alignment: .leading, spacing: 3) {
                ForEach(items) { item in
                    row(item)
                }
            }
        }
        .font(.callout)
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.accent.opacity(0.08), in: .rect(cornerRadius: 8))
        .overlay { RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.accent.opacity(0.28)) }
        .padding(.horizontal, 10)
        .padding(.bottom, 10)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Onboarding.checklistTitle)
    }

    private func row(_ item: Onboarding.ChecklistItem) -> some View {
        HStack(spacing: 8) {
            ZStack {
                if item.isDone {
                    Circle().fill(Theme.accent)
                    Image(systemName: "checkmark")
                        .font(.system(size: 7, weight: .heavy))
                        .foregroundStyle(Theme.onAccent)
                } else {
                    Circle().strokeBorder(Color.secondary, lineWidth: 1.5)
                }
            }
            .frame(width: 13, height: 13)
            .accessibilityHidden(true)
            if item.isDone {
                Text(item.title)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("\(item.title), done")
            } else {
                Button { act(item) } label: {
                    (Text(item.title).fontWeight(.medium)
                        + Text(item.isOptional ? " (optional)" : "").foregroundStyle(.secondary))
                }
                .buttonStyle(.plain)
                .pointerStyle(.link)
            }
        }
        .font(.system(size: 12))
        .frame(minHeight: 20)
    }

    /// Takes the user where the row is done: Detect Services, the Project to start, or Connect Your Agents.
    private func act(_ item: Onboarding.ChecklistItem) {
        let selected = model.projects.first { $0.id == model.selectedProjectID }
        switch item.step {
        case .project:
            model.presentedSheet = .addProject
        case .service:
            let target = selected?.services.isEmpty == true ? selected : model.projects.first { $0.services.isEmpty }
            if let target { model.presentedSheet = .detectServices(projectID: target.id) }
        case .start:
            let target = selected?.services.isEmpty == false ? selected : model.projects.first { !$0.services.isEmpty }
            if let target {
                model.sidebarSelection = .project(target.id)
                model.focusColumn(.list)
            }
        case .agents:
            model.showConnectAgents()
        }
    }
}
