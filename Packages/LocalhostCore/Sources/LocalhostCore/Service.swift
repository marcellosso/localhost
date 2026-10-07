import Foundation

/// One independently manageable long-running command belonging to a Project.
///
/// The command line is run through the user's login shell (ADR-0003), so
/// there is no separate arguments field.
public struct Service: Identifiable, Hashable, Sendable, Codable {
    public let id: UUID
    public internal(set) var name: String
    public internal(set) var key: Key
    /// Relative to the Project's path; `.` is the Project root.
    public internal(set) var workingDirectory: String
    public internal(set) var commandLine: String
    public internal(set) var environment: [String: String]
    /// The Services of the same Project this one needs before it can start.
    public internal(set) var dependencies: [UUID]
    /// Start All skips this Service, unless a Service it starts depends on it.
    public internal(set) var isExcludedFromStartAll: Bool

    init(
        id: UUID = UUID(),
        name: String,
        key: Key,
        workingDirectory: String,
        commandLine: String,
        environment: [String: String] = [:],
        dependencies: [UUID] = [],
        isExcludedFromStartAll: Bool = false
    ) {
        self.id = id
        self.name = name
        self.key = key
        self.workingDirectory = workingDirectory
        self.commandLine = commandLine
        self.environment = environment
        self.dependencies = dependencies
        self.isExcludedFromStartAll = isExcludedFromStartAll
    }

    private enum CodingKeys: String, CodingKey {
        case id, name, key, workingDirectory, commandLine, environment, dependencies, isExcludedFromStartAll
    }

    /// Fields added after schema version 1 shipped are optional when reading.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        key = try container.decode(Key.self, forKey: .key)
        workingDirectory = try container.decode(String.self, forKey: .workingDirectory)
        commandLine = try container.decode(String.self, forKey: .commandLine)
        environment = try container.decode([String: String].self, forKey: .environment)
        dependencies = try container.decodeIfPresent([UUID].self, forKey: .dependencies) ?? []
        isExcludedFromStartAll = try container.decodeIfPresent(Bool.self, forKey: .isExcludedFromStartAll) ?? false
    }
}
