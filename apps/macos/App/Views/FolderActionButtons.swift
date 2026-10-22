import LocalhostCore
import SwiftUI

/// Finder / editor / Terminal for one folder, as context menu items.
struct FolderActionButtons: View {
    @Environment(AppModel.self) private var model
    let folder: URL

    var body: some View {
        Button("Reveal in Finder") { model.revealInFinder(folder) }
        Button("Open in Editor") { model.openInEditor(folder) }
        Button("Open in Terminal") { model.openTerminal(at: folder) }
    }
}

/// The same actions in the Project menu, on the main window's selection:
/// Finder and editor open the Project, Terminal opens at the selected
/// Service's working directory (else the Project's folder).
struct FolderActionCommands: View {
    let model: AppModel

    var body: some View {
        let project = model.selectedProject
        Button("Reveal in Finder") { project.map { model.revealInFinder(AppModel.folder(of: $0)) } }
            .keyboardShortcut("f", modifiers: [.command, .option])
            .disabled(project == nil)
        Button("Open in Editor") { project.map { model.openInEditor(AppModel.folder(of: $0)) } }
            .keyboardShortcut("e", modifiers: [.command, .option])
            .disabled(project == nil)
        Button(model.selectedService == nil ? "Open in Terminal" : "Open Service in Terminal") {
            model.selectedFolder.map(model.openTerminal)
        }
        .keyboardShortcut("t", modifiers: [.command, .option])
        .disabled(project == nil)
    }
}
