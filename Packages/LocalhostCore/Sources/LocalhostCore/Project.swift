import Foundation

/// One local development project, rooted at a folder on the developer's
/// machine, grouping the Services needed to work on it.
///
/// Projects are created and changed only through `ProjectLibrary`, which
/// validates them, so every `Project` a client sees is valid.
public struct Project: Identifiable, Hashable, Sendable, Codable {
    public let id: UUID
    public internal(set) var name: String
    public internal(set) var key: Key
    /// Absolute path of the Project's root folder.
    public internal(set) var path: String
    public internal(set) var sortOrder: Int
    public internal(set) var isFavorite: Bool
    public internal(set) var services: [Service]

    init(
        id: UUID = UUID(),
        name: String,
        key: Key,
        path: String,
        sortOrder: Int,
        isFavorite: Bool = false,
        services: [Service] = []
    ) {
        self.id = id
        self.name = name
        self.key = key
        self.path = path
        self.sortOrder = sortOrder
        self.isFavorite = isFavorite
        self.services = services
    }

    /// The absolute working directory a Service runs in.
    public func workingDirectoryURL(for service: Service) -> URL {
        URL(filePath: path, directoryHint: .isDirectory)
            .appending(path: service.workingDirectory, directoryHint: .isDirectory)
            .standardizedFileURL
    }
}
