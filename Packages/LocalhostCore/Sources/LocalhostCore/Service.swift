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

    init(
        id: UUID = UUID(),
        name: String,
        key: Key,
        workingDirectory: String,
        commandLine: String,
        environment: [String: String] = [:]
    ) {
        self.id = id
        self.name = name
        self.key = key
        self.workingDirectory = workingDirectory
        self.commandLine = commandLine
        self.environment = environment
    }
}
