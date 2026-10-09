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
        Form {
            Section {
                VariableEditor(variables: $variables, layer: .project(project.id))
            } header: {
                Text("Project Variables")
            } footer: {
                Text("Every service of this project gets these. A service’s own variables override them.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            ForEach(profiles) { draft in
                let binding = $profiles.element(draft)
                let profile = binding.wrappedValue
                Section {
                    VariableEditor(variables: binding.variables, layer: profile.profileID.map { .profile($0) })
                } header: {
                    HStack {
                        TextField("Profile Name", text: binding.name, prompt: Text("Testing"))
                            .labelsHidden()
                            .textFieldStyle(.plain)
                            .font(.headline)
                        Spacer()
                        Button("Remove Profile", systemImage: "trash", role: .destructive) {
                            profiles.removeAll { $0.id == profile.id }
                            if activeDraftID == profile.id { activeDraftID = nil }
                        }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                        .help("Remove this profile")
                    }
                }
            }
            Section {
                Button("Add Profile", systemImage: "plus") {
                    profiles.append(AppModel.ProfileDraft(name: suggestedProfileName, variables: []))
                }
                .buttonStyle(.borderless)
                Picker("Active Profile", selection: $activeDraftID) {
                    Text("None").tag(UUID?.none)
                    ForEach(profiles) { profile in
                        Text(profile.name.isEmpty ? "Untitled" : profile.name).tag(UUID?.some(profile.id))
                    }
                }
            } header: {
                Text("Environment Profiles")
            } footer: {
                Text("The active profile’s variables override the project’s and each service’s. Running services show “Restart to apply” when their environment changes.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
        }
        .formStyle(.grouped)
        .sheetTitle("\(project.name) Environment")
        .frame(width: 560)
        .frame(minHeight: 420)
        .navigationTitle("\(project.name) Environment")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") { Task { await save() } }
                    .disabled(isSaving)
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
