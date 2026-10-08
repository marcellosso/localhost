import Foundation
import Security

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
    case duplicateVariableName(String)
    case duplicateProfileName(String)
    case invalidPort(Int)
    case invalidURL(String)
    case duplicateProjectKey(Key)
    case duplicateServiceKey(Key, projectID: UUID)
    /// The Dependency would make a Service depend on itself, e.g. `[api, worker, api]`.
    case dependencyCycle([Key])

    // Lookup
    case projectNotFound(UUID)
    case serviceNotFound(UUID)
    case profileNotFound(UUID)

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
    /// A Secret of the Run's environment has no stored value.
    case secretMissing(String)
    /// The Run failed after printing `Cannot connect to the Docker daemon`.
    case dockerDaemonUnavailable

    // Ports and Listeners
    /// Something already listens on the port; `owner` is that Listener, if
    /// localhost can see it (processes of other users can't be inspected).
    case portConflict(port: Int, owner: Listener?)
    /// localhost does not stop itself.
    case listenerIsLocalhost
    /// launchd would restart it (a Homebrew service, an app): stop it there instead.
    case listenerIsLaunchdJob(label: String)
    /// Docker's backend holds the ports of every running container.
    case listenerIsDocker

    // Persistence
    /// A Keychain call failed with this `OSStatus`.
    case keychainFailed(status: Int32)
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
        case .duplicateVariableName(let name):
            "“\(name)” is defined more than once."
        case .duplicateProfileName(let name):
            "Another profile in this project is already called “\(name)”."
        case .invalidPort(let port):
            "\(port) is not a valid port. Use a number from 1 to 65535."
        case .invalidURL(let raw):
            "“\(raw)” is not a valid URL. Use a full http:// or https:// address."
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
        case .profileNotFound:
            "The environment profile no longer exists."
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
        case .secretMissing(let name):
            "The secret “\(name)” has no stored value. Set it again."
        case .dockerDaemonUnavailable:
            "Docker isn’t running. Start Docker Desktop (or your Docker engine), then start the service again."
        case .portConflict(let port, let owner?):
            "Port \(port) is in use by \(owner.name) (PID \(owner.pid))."
        case .portConflict(let port, nil):
            "Port \(port) is in use by another process."
        case .listenerIsLocalhost:
            "localhost can't stop itself."
        case .listenerIsLaunchdJob(let label):
            "It is managed by launchd (\(label)), which would start it again. Stop it with the tool that started it."
        case .listenerIsDocker:
            "This is Docker listening for a container. Stop the container instead."
        case .unsupportedSchemaVersion(let version, let path):
            "\(path) was saved by a newer version of localhost (schema \(version))."
        case .keychainFailed(let status):
            "The Keychain refused: \((SecCopyErrorMessageString(status, nil) as String?) ?? "error \(status)")."
        case .persistenceFailed(let path, let reason):
            "Could not read or write \(path): \(reason)"
        }
    }
}
