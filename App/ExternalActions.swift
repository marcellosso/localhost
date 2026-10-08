import AppKit
import LocalhostCore
import UniformTypeIdentifiers

/// Finder, editor and Terminal actions on a Project or Service folder.
/// App-level only: the core knows the folders, not the apps that open them.
extension AppModel {
    /// The editor picked in Settings; `nil` until the user picks one (there is no default).
    static var editorApplicationURL: URL? {
        get {
            UserDefaults.standard.string(forKey: editorApplicationKey).map { URL(filePath: $0, directoryHint: .isDirectory) }
        }
        set { UserDefaults.standard.set(newValue?.path, forKey: editorApplicationKey) }
    }

    static let editorApplicationKey = "editorApplicationPath"

    /// The folder actions on the main window's selection use: the selected
    /// Service's working directory, else the selected Project's folder.
    var selectedFolder: URL? {
        guard let project = selectedProject else { return nil }
        if let service = selectedService { return project.workingDirectoryURL(for: service) }
        return Self.folder(of: project)
    }

    static func folder(of project: Project) -> URL {
        URL(filePath: project.path, directoryHint: .isDirectory).standardizedFileURL
    }

    func folder(ofService serviceID: UUID) -> URL? {
        service(serviceID).map { $0.project.workingDirectoryURL(for: $0.service) }
    }

    func revealInFinder(_ folder: URL) {
        guard exists(folder) else { return }
        NSWorkspace.shared.activateFileViewerSelecting([folder])
    }

    /// Opens the folder in the editor picked in Settings, asking for one first if none is.
    func openInEditor(_ folder: URL) {
        guard exists(folder) else { return }
        guard let editor = Self.editorApplicationURL ?? Self.chooseEditor() else { return }
        open(folder, with: editor)
    }

    /// Opens a new Terminal window at the folder.
    func openTerminal(at folder: URL) {
        guard exists(folder) else { return }
        guard let terminal = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Terminal") else { return }
        open(folder, with: terminal)
    }

    /// Asks the user to pick the editor app and remembers it; `nil` if they cancel.
    @discardableResult
    static func chooseEditor() -> URL? {
        let panel = NSOpenPanel()
        panel.title = "Choose Editor"
        panel.message = "localhost opens projects in this app."
        panel.prompt = "Choose"
        panel.directoryURL = URL(filePath: "/Applications", directoryHint: .isDirectory)
        panel.allowedContentTypes = [.application]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        editorApplicationURL = url
        return url
    }

    private func exists(_ folder: URL) -> Bool {
        var isDirectory: ObjCBool = false
        if FileManager.default.fileExists(atPath: folder.path, isDirectory: &isDirectory), isDirectory.boolValue {
            return true
        }
        actionError = .workingDirectoryMissing(folder.path)
        return false
    }

    private func open(_ folder: URL, with application: URL) {
        NSWorkspace.shared.open([folder], withApplicationAt: application, configuration: NSWorkspace.OpenConfiguration()) { _, error in
            guard let error else { return }
            Task { @MainActor in NSAlert(error: error).runModal() }
        }
    }
}
