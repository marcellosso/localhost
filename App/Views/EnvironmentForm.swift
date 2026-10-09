import LocalhostCore
import SwiftUI

/// Edits a Project's variables and its Environment Profiles, and which
/// Profile is active.
struct EnvironmentForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let project: Project

    @State private var variables: [VariableDraft] = []
    @State private var profiles: [AppModel.ProfileDraft] = []
    @State private var activeDraftID: UUID?
    @State private var error: String?
    @State private var isSaving = false

    var body: some View {
        FormSheet(
            systemImage: "square.3.layers.3d",
            title: "Environment",
            subtitle: Text("for **\(project.name)**"),
            hint: "Running services show “restart to apply” when their environment changes.",
            error: error,
            primaryTitle: "Save",
            isPrimaryDisabled: isSaving,
            width: 600,
            cancel: { dismiss() },
            primary: { Task { await save() } }
        ) {
            Field(title: "Project variables", note: "every service gets these",
                  hint: "A service’s own variables override them.") {
                VariableEditor(variables: $variables, layer: .project(project.id))
            }
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    FieldLabel(title: "Profiles", note: nil)
                    Spacer()
                    Button("Add Profile", systemImage: "plus") {
                        profiles.append(AppModel.ProfileDraft(name: suggestedProfileName, variables: []))
                    }
                    .buttonStyle(PillButtonStyle(isCompact: true))
                }
                if profiles.isEmpty {
                    FieldHint("A profile is a named set of variables, like “Testing”, that overrides the project’s and each service’s while it is active.")
                } else {
                    Field(title: "Active profile") {
                        FlowLayout {
                            ChipToggle(title: "None", color: .secondary, isOn: activeBinding(nil))
                            ForEach(profiles) { profile in
                                ChipToggle(title: profile.name.isEmpty ? "Untitled" : profile.name,
                                           color: Theme.accent, isOn: activeBinding(profile.id))
                            }
                        }
                    }
                    ForEach(profiles) { draft in
                        profileCard(draft)
                    }
                }
            }
        }
        .onAppear {
            variables = VariableDraft.drafts(from: project.variables)
            profiles = project.profiles.map { profile in
                AppModel.ProfileDraft(profileID: profile.id, name: profile.name, variables: VariableDraft.drafts(from: profile.variables))
            }
            activeDraftID = profiles.first { $0.profileID == project.activeProfileID && $0.profileID != nil }?.id
        }
    }

    /// Whether the profile (or none, for `nil`) is the active one; choosing it makes it so.
    private func activeBinding(_ id: UUID?) -> Binding<Bool> {
        Binding { activeDraftID == id } set: { if $0 { activeDraftID = id } }
    }

    private func profileCard(_ draft: AppModel.ProfileDraft) -> some View {
        let binding = $profiles.element(draft)
        let profile = binding.wrappedValue
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                TextField("Profile Name", text: binding.name, prompt: Text("Testing"))
                    .labelsHidden()
                    .textFieldStyle(.plain)
                    .font(.system(size: 13, weight: .semibold))
                if activeDraftID == profile.id {
                    Text("Active")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Theme.accentText)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 1)
                        .background(Theme.accent.opacity(0.14), in: .capsule)
                }
                Spacer()
                Button("Remove Profile", systemImage: "trash") {
                    profiles.removeAll { $0.id == profile.id }
                    if activeDraftID == profile.id { activeDraftID = nil }
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .help("Remove this profile")
            }
            VariableEditor(variables: binding.variables, layer: profile.profileID.map { .profile($0) })
        }
    }

    private var suggestedProfileName: String {
        let names = Set(profiles.map(\.name))
        return ["Development", "Testing", "Staging", "Production"].first { !names.contains($0) } ?? "Profile \(profiles.count + 1)"
    }

    private func save() async {
        isSaving = true
        defer { isSaving = false }
        do {
            try await model.saveEnvironment(
                of: project.id, variables: variables.compactMap(\.input), profiles: profiles, activeDraftID: activeDraftID
            )
            dismiss()
        } catch {
            self.error = error.message
        }
    }
}
