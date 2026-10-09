import LocalhostCore
import SwiftUI
import UniformTypeIdentifiers

/// Adds a Project (folder, name, key derived from the name) or edits one.
struct ProjectForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let editing: Project?

    @State private var path = ""
    @State private var name = ""
    @State private var key = ""
    /// Once the user edits the key it stops following the name.
    @State private var isKeyEdited = false
    @State private var isChoosingFolder = false
    @State private var error: LocalhostError?
    @State private var isSaving = false

    var body: some View {
        FormSheet(
            systemImage: "folder",
            title: editing == nil ? "New Project" : "Edit \(editing?.name ?? "Project")",
            subtitle: Text("A folder whose services localhost runs."),
            hint: "↩ to \(editing == nil ? "add" : "save")",
            error: error?.message,
            primaryTitle: editing == nil ? "Add Project" : "Save",
            isPrimaryDisabled: path.isEmpty || name.isEmpty || key.isEmpty || isSaving,
            width: 500,
            cancel: { dismiss() },
            primary: { Task { await save() } }
        ) {
            Field(title: "Folder") {
                Button { isChoosingFolder = true } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "folder")
                            .foregroundStyle(.secondary)
                        Text(path.isEmpty ? "Choose a folder…" : (path as NSString).abbreviatingWithTildeInPath)
                            .font(.system(size: 12.5, design: .monospaced))
                            .foregroundStyle(path.isEmpty ? .secondary : .primary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer(minLength: 8)
                        Text("Choose…")
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 10)
                    .frame(height: 32)
                    .fieldBox(isFocused: false)
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
            }
            HStack(alignment: .top, spacing: 12) {
                Field(title: "Name") {
                    BoxedTextField(label: "Name", text: $name, prompt: "My App")
                }
                Field(title: "Key", note: "in logs & CLI") {
                    BoxedTextField(label: "Key", text: keyBinding, prompt: "my-app", isMonospaced: true)
                }
                .frame(width: 160)
            }
            .onChange(of: name) {
                if !isKeyEdited { key = model.suggestedProjectKey(for: name, excluding: editing?.id) }
            }
        }
        .fileImporter(isPresented: $isChoosingFolder, allowedContentTypes: [.folder]) { result in
            guard case .success(let url) = result else { return }
            path = url.path
            if name.isEmpty { name = url.lastPathComponent }
        }
        .onAppear {
            guard let editing else { return }
            path = editing.path
            name = editing.name
            key = editing.key.rawValue
            isKeyEdited = true
        }
    }

    private var keyBinding: Binding<String> {
        Binding { key } set: { newValue in
            key = newValue
            isKeyEdited = true
        }
    }

    private func save() async {
        isSaving = true
        defer { isSaving = false }
        do {
            if let editing {
                try await model.editProject(editing.id, name: name, key: key, path: path)
            } else {
                try await model.addProject(name: name, key: key, path: path)
            }
            dismiss()
        } catch {
            self.error = error
        }
    }
}
