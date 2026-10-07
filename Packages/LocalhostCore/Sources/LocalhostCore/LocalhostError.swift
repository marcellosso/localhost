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
    /// The Dependency would make a Service depend on itself, e.g. `[api, worker, api]`.
    case dependencyCycle([Key])

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
    /// A Dependency Failed, so the Service was not started.
    case dependencyFailed(Key)

    // Persistence
    case unsupportedSchemaVersion(Int, path: String)
    case persistenceFailed(path: String, reason: String)
}

extension LocalhostError {
    /// A short, user-facing explanation.
    public var message: String {
        switch self {
        case .invalidKey(let raw):
            "“\(raw)” is not a valid key. Use lowercase letters, digits and single hyphens, e.g. my-app."
        case .invalidName:
            "Enter a name."
        case .invalidProjectPath:
            "The project folder must be an absolute path."
        case .invalidWorkingDirectory:
            "The working directory must be relative to the project folder."
        case .emptyCommandLine:
            "Enter a command line."
        case .invalidEnvironmentVariableName(let name):
            "“\(name)” is not a valid environment variable name."
        case .duplicateProjectKey(let key):
            "Another project already uses the key “\(key)”."
        case .duplicateServiceKey(let key, _):
            "Another service in this project already uses the key “\(key)”."
        case .dependencyCycle(let keys):
            "\(keys.map(\.rawValue).joined(separator: " → ")) would be a dependency cycle."
        case .projectNotFound:
            "The project no longer exists."
        case .serviceNotFound:
            "The service no longer exists."
        case .workingDirectoryMissing(let path):
            "The working directory \(path) does not exist."
        case .shellNotFound(let path):
            "The shell \(path) was not found."
        case .launchFailed(let code):
            "The command could not be launched: \(String(cString: strerror(code)))."
        case .commandNotFound:
            "Command not found."
        case .commandNotExecutable:
            "The command is not executable."
        case .dependencyFailed(let key):
            "Not started: its dependency “\(key)” failed."
        case .unsupportedSchemaVersion(let version, let path):
            "\(path) was saved by a newer version of localhost (schema \(version))."
        case .persistenceFailed(let path, let reason):
            "Could not read or write \(path): \(reason)"
        }
    }
}
