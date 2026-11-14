import LocalhostCore
import SwiftUI
import UniformTypeIdentifiers

/// Adds a Project (folder, name, key derived from the name) or edits one.
/// When adding, the folder is scanned as soon as it is chosen and the
/// Suggestions are reviewed in the same sheet, then saved with the Project.
/// On another Machine, the folder is chosen and scanned there.
struct ProjectForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let editing: Project?
    var place: AppModel.Place = .thisMac

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
            title: title,
            subtitle: subtitle,
            hint: hint,
            error: error,
            primaryTitle: editing == nil ? "Add Project" : "Save",
            isPrimaryDisabled: path.isEmpty || name.isEmpty || key.isEmpty || isScanning || isSaving || !model.canChange(place),
            width: editing == nil && !path.isEmpty ? 640 : 500,
            cancel: { dismiss() },
            primary: { Task { await save() } }
        ) {
            Field(title: "Folder") {
                FolderButton(path: path, isOnThisMac: place == .thisMac) { isChoosingFolder = true }
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
                if !isKeyEdited { key = model.suggestedProjectKey(for: name, excluding: editing?.id, on: place) }
            }
            if editing == nil, !path.isEmpty {
                SuggestionReview(
                    drafts: $drafts, isScanning: isScanning, projectPath: path, place: place,
                    emptyMessage: "Nothing recognised in this folder. Add the project, then add its services yourself."
                )
            }
        }
        .fileImporter(isPresented: isChoosingFolderHere, allowedContentTypes: [.folder]) { result in
            guard case .success(let url) = result else { return }
            choose(url.path)
        }
        .sheet(isPresented: isChoosingFolderThere) {
            if case .machine(let machineID) = place {
                RemoteFolderPicker(machineID: machineID, startingAt: path.isEmpty ? nil : path, choose: choose)
            }
        }
        .onAppear {
            guard let editing else { return }
            path = editing.path
            name = editing.name
            key = editing.key.rawValue
            isKeyEdited = true
        }
    }

    private var title: String {
        if let editing { return "Edit \(editing.name)" }
        return model.machineName(place).map { "New Project on \($0)" } ?? "New Project"
    }

    /// Another Machine's form says which it is, and for an edit, where the Project is there.
    private var subtitle: Text {
        guard let machine = model.machineName(place) else { return Text("A folder whose services localhost runs.") }
        guard let editing else { return Text("A folder on \(machine) whose services localhost runs.") }
        return Text("on \(machine) · ") + Text(editing.path).monospaced()
    }

    /// This Mac's folders are chosen in the open panel, another Machine's in its own picker.
    private var isChoosingFolderHere: Binding<Bool> {
        Binding { isChoosingFolder && place == .thisMac } set: { isChoosingFolder = $0 }
    }

    private var isChoosingFolderThere: Binding<Bool> {
        Binding { isChoosingFolder && place != .thisMac } set: { isChoosingFolder = $0 }
    }

    /// The folder's last component names the Project until the user does.
    private func choose(_ folder: String) {
        path = folder
        if name.isEmpty { name = folder.split(separator: "/").last.map(String.init) ?? "" }
        if editing == nil { scan() }
    }

    private var keyBinding: Binding<String> {
        Binding { key } set: { newValue in
            key = newValue
            isKeyEdited = true
        }
    }

    private var hint: String {
        if let reason = model.reasonCantChange(place) { return reason }
        guard editing == nil else { return "↩ to save" }
        let count = drafts.filter(\.isChecked).count
        return count == 0 ? "↩ to add" : "↩ to add with \(count) service\(count == 1 ? "" : "s")"
    }

    private func scan() {
        let folder = path
        scannedPath = folder
        drafts = []
        error = nil
        isScanning = true
        Task {
            let suggestions: [ServiceSuggestion]
            do throws(LocalhostError) {
                suggestions = try await model.suggestions(forFolderAt: folder, on: place)
            } catch {
                guard scannedPath == folder else { return }
                self.error = error.message
                suggestions = []
            }
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
                try await model.editProject(editing.id, name: name, key: key, path: path, on: place)
            } else {
                try await model.addProject(name: name, key: key, path: path, services: services, on: place)
            }
            dismiss()
        } catch {
            self.error = error.message
        }
    }
}
