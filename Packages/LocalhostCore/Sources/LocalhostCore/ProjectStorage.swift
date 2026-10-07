import Foundation

/// Reads and writes Project Configuration: one JSON file per Project at
/// `<root>/projects/<project-uuid>.json` (ADR-0004).
struct ProjectStorage: Sendable {
    static let schemaVersion = 1

    let projectsDirectory: URL

    init(root: URL) {
        projectsDirectory = root.appending(path: "projects", directoryHint: .isDirectory)
    }

    func fileURL(for id: UUID) -> URL {
        projectsDirectory.appending(path: "\(id.uuidString).json", directoryHint: .notDirectory)
    }

    func loadAll() throws(LocalhostError) -> [Project] {
        try createDirectoryIfNeeded()
        let fileNames: [String]
        do {
            fileNames = try FileManager.default.contentsOfDirectory(atPath: projectsDirectory.path)
        } catch {
            throw .persistenceFailed(path: projectsDirectory.path, reason: error.localizedDescription)
        }
        var projects: [Project] = []
        for fileName in fileNames where fileName.hasSuffix(".json") {
            projects.append(try load(projectsDirectory.appending(path: fileName, directoryHint: .notDirectory)))
        }
        return projects
    }

    func save(_ project: Project) throws(LocalhostError) {
        try createDirectoryIfNeeded()
        let url = fileURL(for: project.id)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        do {
            let data = try encoder.encode(File(schemaVersion: Self.schemaVersion, project: project))
            try data.write(to: url, options: .atomic)
        } catch {
            throw .persistenceFailed(path: url.path, reason: error.localizedDescription)
        }
    }

    func delete(_ id: UUID) throws(LocalhostError) {
        let url = fileURL(for: id)
        do {
            try FileManager.default.removeItem(at: url)
        } catch CocoaError.fileNoSuchFile {
            return
        } catch {
            throw .persistenceFailed(path: url.path, reason: error.localizedDescription)
        }
    }

    private func load(_ url: URL) throws(LocalhostError) -> Project {
        do {
            let data = try Data(contentsOf: url)
            let decoder = JSONDecoder()
            let version = try decoder.decode(Version.self, from: data).schemaVersion
            guard version == Self.schemaVersion else {
                throw LocalhostError.unsupportedSchemaVersion(version, path: url.path)
            }
            return try decoder.decode(File.self, from: data).project
        } catch let error as LocalhostError {
            throw error
        } catch {
            throw .persistenceFailed(path: url.path, reason: String(describing: error))
        }
    }

    private func createDirectoryIfNeeded() throws(LocalhostError) {
        do {
            try FileManager.default.createDirectory(at: projectsDirectory, withIntermediateDirectories: true)
        } catch {
            throw .persistenceFailed(path: projectsDirectory.path, reason: error.localizedDescription)
        }
    }

    private struct Version: Decodable {
        let schemaVersion: Int
    }

    private struct File: Codable {
        let schemaVersion: Int
        let project: Project
    }
}
