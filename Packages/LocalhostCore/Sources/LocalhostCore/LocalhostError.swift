import Foundation

/// Every failure the core reports. Clients switch over these cases; they are
/// never flattened into strings inside the core.
public enum LocalhostError: Error, Sendable, Equatable {
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

    // Persistence
    case unsupportedSchemaVersion(Int, path: String)
    case persistenceFailed(path: String, reason: String)
}
