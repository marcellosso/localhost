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
        Form {
            LabeledContent("Folder") {
                HStack {
                    Text(path.isEmpty ? "No folder chosen" : path)
                        .font(.callout.monospaced())
                        .foregroundStyle(path.isEmpty ? .secondary : .primary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer()
                    Button("Choose…") { isChoosingFolder = true }
                }
            }
            TextField("Name", text: $name, prompt: Text("My App"))
                .onChange(of: name) {
                    if !isKeyEdited { key = model.suggestedProjectKey(for: name, excluding: editing?.id) }
                }
            MonospacedField("Key", text: keyBinding, prompt: "my-app")
            if let error {
                Text(error.message)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
        }
        .formStyle(.grouped)
        .sheetTitle(editing == nil ? "Add Project" : "Edit Project")
        .frame(width: 480)
        .navigationTitle(editing == nil ? "Add Project" : "Edit Project")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(editing == nil ? "Add" : "Save") { Task { await save() } }
                    .disabled(path.isEmpty || name.isEmpty || key.isEmpty || isSaving)
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
