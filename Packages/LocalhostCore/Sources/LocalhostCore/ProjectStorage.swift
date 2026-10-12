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

    /// Every Project that can be loaded, plus the files that can't. A file
    /// written by a newer localhost is left in place for it; any other
    /// unreadable file (damaged, or a second copy of a Project or its Key) is
    /// moved aside as `<name>.unreadable`, so it is kept but never loaded or
    /// overwritten. Only an unreadable folder fails the whole load.
    func loadAll() throws(LocalhostError) -> (projects: [Project], unreadable: [UnreadableProjectFile]) {
        try createDirectoryIfNeeded()
        let fileNames: [String]
        do {
            fileNames = try FileManager.default.contentsOfDirectory(atPath: projectsDirectory.path)
        } catch {
            throw .persistenceFailed(path: projectsDirectory.path, reason: error.localizedDescription)
        }
        var loaded: [(url: URL, project: Project)] = []
        var unreadable: [UnreadableProjectFile] = []
        for fileName in fileNames where fileName.hasSuffix(".json") {
            let url = projectsDirectory.appending(path: fileName, directoryHint: .notDirectory)
            do throws(LocalhostError) {
                loaded.append((url, try load(url)))
            } catch {
                if case .unsupportedSchemaVersion = error {
                    unreadable.append(UnreadableProjectFile(path: url.path, movedTo: nil, error: error))
                } else {
                    unreadable.append(UnreadableProjectFile(path: url.path, movedTo: moveAside(url), error: error))
                }
            }
        }
        // Of two copies, the file saves write to (`<id>.json`) wins, then the
        // first by name, so the outcome doesn't depend on the folder's order.
        loaded.sort { a, b in
            let (aIsOwn, bIsOwn) = (a.url == fileURL(for: a.project.id), b.url == fileURL(for: b.project.id))
            return aIsOwn != bIsOwn ? aIsOwn : a.url.lastPathComponent < b.url.lastPathComponent
        }
        var projects: [Project] = []
        for (url, project) in loaded {
            let error: LocalhostError? = if let original = projects.first(where: { $0.id == project.id }) {
                .persistenceFailed(path: url.path, reason: "It is a second copy of the project “\(original.name)”.")
            } else if projects.contains(where: { $0.key == project.key }) {
                .duplicateProjectKey(project.key)
            } else {
                nil
            }
            if let error {
                unreadable.append(UnreadableProjectFile(path: url.path, movedTo: moveAside(url), error: error))
            } else {
                projects.append(project)
            }
        }
        return (projects, unreadable)
    }

    /// Renames the file to `<name>.unreadable` (or `.unreadable-2`, …).
    /// Returns the new path, or `nil` if it couldn't be moved.
    private func moveAside(_ url: URL) -> String? {
        var suffix = 1
        while true {
            let destination = URL(filePath: url.path + (suffix == 1 ? ".unreadable" : ".unreadable-\(suffix)"))
            do {
                try FileManager.default.moveItem(at: url, to: destination)
                return destination.path
            } catch CocoaError.fileWriteFileExists {
                suffix += 1
            } catch {
                return nil
            }
        }
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
        } catch let error as DecodingError {
            throw .persistenceFailed(path: url.path, reason: Self.describe(error))
        } catch {
            throw .persistenceFailed(path: url.path, reason: error.localizedDescription)
        }
    }

    /// Why a Project file doesn't decode, in a sentence.
    private static func describe(_ error: DecodingError) -> String {
        func field(_ path: [any CodingKey]) -> String {
            path.map { $0.intValue.map { "[\($0)]" } ?? $0.stringValue }.joined(separator: ".")
        }
        switch error {
        case .keyNotFound(let key, let context):
            return "It is damaged: “\(field(context.codingPath + [key]))” is missing."
        case .typeMismatch(_, let context), .valueNotFound(_, let context):
            return "It is damaged: “\(field(context.codingPath))” has the wrong type."
        case .dataCorrupted(let context) where context.codingPath.isEmpty:
            return "It is damaged: it isn’t valid JSON."
        case .dataCorrupted(let context):
            return "It is damaged: “\(field(context.codingPath))”: \(context.debugDescription)"
        @unknown default:
            return "It is damaged."
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

/// A Project file that couldn't be loaded at launch, and why.
public struct UnreadableProjectFile: Sendable, Hashable {
    public let path: String
    /// Where the file was moved aside to; `nil` if it was left in place
    /// (written by a newer localhost) or couldn't be moved.
    public let movedTo: String?
    public let error: LocalhostError
}
