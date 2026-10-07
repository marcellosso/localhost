import Foundation

/// Every failure the core reports. Clients switch over these cases; they are
/// never flattened into strings inside the core.
public enum LocalhostError: Error, Sendable, Hashable, Codable {
    // Validation
    case invalidKey(String)
    case invalidName
    case invalidProjectPath(String)
    case invalidWorkingDirectory(String)
    case emptyCommandLine
    case invalidEnvironmentVariableName(String)
    case duplicateProjectKey(Key)
    case duplicateServiceKey(Key, projectID: UUID)

    // Lookup
    case projectNotFound(UUID)
    case serviceNotFound(UUID)

    // Launching a Run
    case workingDirectoryMissing(String)
    case shellNotFound(String)
    case launchFailed(errno: Int32)
    /// The shell exited with 127: the command was not found.
    case commandNotFound
    /// The shell exited with 126: the command was found but could not be executed.
    case commandNotExecutable

    // Persistence
    case unsupportedSchemaVersion(Int, path: String)
    case persistenceFailed(path: String, reason: String)
}
