import Foundation
import LocalhostCore
import SwiftUI
import UniformTypeIdentifiers

/// A Project file as the save panel writes it.
nonisolated struct ProjectFile: FileDocument {
    static let readableContentTypes: [UTType] = [.yaml]

    let text: String
    /// e.g. `my-saas.yaml`.
    let defaultFileName: String

    init(_ document: ProjectDocument) {
        text = document.yaml
        defaultFileName = "\(document.key).yaml"
    }

    init(configuration: ReadConfiguration) throws {
        text = String(decoding: configuration.file.regularFileContents ?? Data(), as: UTF8.self)
        defaultFileName = configuration.file.filename ?? "project.yaml"
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: Data(text.utf8))
    }
}

/// Export… and Import…: one YAML file per Project. Import always goes
/// through a review sheet; nothing is overwritten without Replace.
extension AppModel {
    struct ImportFailure: Identifiable {
        let id = UUID()
        let fileName: String
        let error: LocalhostError
    }

    /// Asks where to save the Project's file.
    func exportProject(_ id: UUID) {
        guard let project = project(id) else { return }
        exportingFile = ProjectFile(ProjectDocument(project))
    }

    /// Reads a chosen Project file and opens its review sheet, or says why it can't be imported.
    func openProjectFile(at url: URL) {
        let fileName = url.lastPathComponent
        let isScoped = url.startAccessingSecurityScopedResource()
        defer { if isScoped { url.stopAccessingSecurityScopedResource() } }
        do throws(LocalhostError) {
            let text: String
            do {
                text = try String(contentsOf: url, encoding: .utf8)
            } catch {
                throw .persistenceFailed(path: url.path, reason: error.localizedDescription)
            }
            presentedSheet = .importProject(try ProjectDocument(yaml: text), fileName: fileName)
        } catch {
            importFailure = ImportFailure(fileName: fileName, error: error)
        }
    }

    /// The Project already using the document's Key, which the import would replace.
    func projectClashing(with document: ProjectDocument) -> Project? {
        projects.first { $0.key == document.key }
    }

    /// The Key "Keep both" would give the import.
    func freeProjectKey(basedOn key: Key) async -> Key {
        await core?.library.freeProjectKey(basedOn: key) ?? key
    }
}
