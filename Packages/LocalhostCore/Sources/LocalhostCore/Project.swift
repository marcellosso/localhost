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
    /// Variables every Service of the Project gets, below its own.
    public internal(set) var variables: Variables
    public internal(set) var profiles: [EnvironmentProfile]
    /// The Environment Profile applied on top of every Service's variables, if any.
    public internal(set) var activeProfileID: UUID?

    init(
        id: UUID = UUID(),
        name: String,
        key: Key,
        path: String,
        sortOrder: Int,
        isFavorite: Bool = false,
        services: [Service] = [],
        variables: Variables = .empty,
        profiles: [EnvironmentProfile] = [],
        activeProfileID: UUID? = nil
    ) {
        self.id = id
        self.name = name
        self.key = key
        self.path = path
        self.sortOrder = sortOrder
        self.isFavorite = isFavorite
        self.services = services
        self.variables = variables
        self.profiles = profiles
        self.activeProfileID = activeProfileID
    }

    private enum CodingKeys: String, CodingKey {
        case id, name, key, path, sortOrder, isFavorite, services, variables, profiles, activeProfileID
    }

    /// Fields added after schema version 1 shipped are optional when reading.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        key = try container.decode(Key.self, forKey: .key)
        path = try container.decode(String.self, forKey: .path)
        sortOrder = try container.decode(Int.self, forKey: .sortOrder)
        isFavorite = try container.decode(Bool.self, forKey: .isFavorite)
        services = try container.decode([Service].self, forKey: .services)
        variables = try container.decodeIfPresent(Variables.self, forKey: .variables) ?? .empty
        profiles = try container.decodeIfPresent([EnvironmentProfile].self, forKey: .profiles) ?? []
        activeProfileID = try container.decodeIfPresent(UUID.self, forKey: .activeProfileID)
    }

    public var activeProfile: EnvironmentProfile? {
        profiles.first { $0.id == activeProfileID }
    }

    public func profile(id: UUID) -> EnvironmentProfile? {
        profiles.first { $0.id == id }
    }

    /// The absolute working directory a Service runs in.
    public func workingDirectoryURL(for service: Service) -> URL {
        URL(filePath: path, directoryHint: .isDirectory)
            .appending(path: service.workingDirectory, directoryHint: .isDirectory)
            .standardizedFileURL
    }
}
