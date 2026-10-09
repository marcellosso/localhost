import LocalhostCore
import SwiftUI
import UniformTypeIdentifiers

/// Adds a Project (folder, name, key derived from the name) or edits one.
/// When adding, the folder is scanned as soon as it is chosen and the
/// Suggestions are reviewed in the same sheet, then saved with the Project.
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
    @State private var error: String?
    @State private var isSaving = false
    @State private var drafts: [SuggestionDraft] = []
    @State private var isScanning = false
    /// The folder the shown Suggestions are for, so a late scan of a
    /// previously chosen folder is ignored.
    @State private var scannedPath = ""

    var body: some View {
        FormSheet(
            systemImage: "folder",
            title: editing == nil ? "New Project" : "Edit \(editing?.name ?? "Project")",
            subtitle: Text("A folder whose services localhost runs."),
            hint: hint,
            error: error,
            primaryTitle: editing == nil ? "Add Project" : "Save",
            isPrimaryDisabled: path.isEmpty || name.isEmpty || key.isEmpty || isScanning || isSaving,
            width: editing == nil && !path.isEmpty ? 640 : 500,
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
            if editing == nil, !path.isEmpty {
                SuggestionReview(
                    drafts: $drafts, isScanning: isScanning, projectPath: path,
                    emptyMessage: "Nothing recognised in this folder. Add the project, then add its services yourself."
                )
            }
        }
        .fileImporter(isPresented: $isChoosingFolder, allowedContentTypes: [.folder]) { result in
            guard case .success(let url) = result else { return }
            path = url.path
            if name.isEmpty { name = url.lastPathComponent }
            if editing == nil { scan() }
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

    private var hint: String {
        guard editing == nil else { return "↩ to save" }
        let count = drafts.filter(\.isChecked).count
        return count == 0 ? "↩ to add" : "↩ to add with \(count) service\(count == 1 ? "" : "s")"
    }

    private func scan() {
        let folder = path
        scannedPath = folder
        drafts = []
        isScanning = true
        Task {
            let suggestions = await model.suggestions(forFolderAt: folder)
            guard scannedPath == folder else { return }
            drafts = suggestions.map(SuggestionDraft.init)
            isScanning = false
        }
    }

    private func save() async {
        let services: [NewService]
        switch SuggestionDraft.newServices(from: editing == nil ? drafts : []) {
        case .success(let new): services = new
        case .failure(let failure):
            error = failure.message
            return
        }
        isSaving = true
        defer { isSaving = false }
        do {
            if let editing {
                try await model.editProject(editing.id, name: name, key: key, path: path)
            } else {
                try await model.addProject(name: name, key: key, path: path, services: services)
            }
            dismiss()
        } catch {
            self.error = error.message
        }
    }
}
