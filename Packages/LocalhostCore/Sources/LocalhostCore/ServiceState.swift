import Foundation

/// The single, unambiguous status of a Service (see `GLOSSARY.md`).
public enum ServiceState: String, Sendable, Hashable, Codable {
    /// Never started, or deliberately stopped, whatever exit code the stop produced.
    case stopped
    /// A Run has been launched but the Service is not yet considered up.
    case starting
    /// A Run is alive and has passed Starting.
    case running
    /// A stop was requested and the Run's process tree has not yet fully ended.
    case stopping
    /// The Run ended on its own with exit code 0, without anyone asking it to stop.
    case exited
    /// The Run ended on its own unsuccessfully, or could not be launched at all.
    case failed

    /// Whether a Run is active (its process tree may be alive).
    public var isActive: Bool {
        switch self {
        case .starting, .running, .stopping: true
        case .stopped, .exited, .failed: false
        }
    }
}

/// One execution of a Service, from launch until its process tree has fully ended.
public struct Run: Identifiable, Sendable, Hashable, Codable {
    public let id: UUID
    public let serviceID: UUID
    /// Who started the Run.
    public let origin: Origin
    public let startedAt: Date
    /// When the process tree fully ended; `nil` while it is still alive.
    public internal(set) var endedAt: Date?
    /// How the command's shell ended; `nil` until it has, or if it never launched.
    public internal(set) var termination: Termination?
    /// Why the Run could not launch or run its command (e.g. `commandNotFound`).
    public internal(set) var failure: LocalhostError?
    /// Who asked the Run to stop, if anyone did.
    public internal(set) var stopOrigin: Origin?
    /// Whether the process group had to be killed with SIGKILL.
    public internal(set) var wasForceKilled = false
}

/// A Service's state together with its active Run, or its most recent one.
public struct ServiceStatus: Sendable, Hashable, Codable {
    public let serviceID: UUID
    public internal(set) var state: ServiceState
    public internal(set) var run: Run?

    public static func neverRun(_ serviceID: UUID) -> ServiceStatus {
        ServiceStatus(serviceID: serviceID, state: .stopped, run: nil)
    }
}

/// A change in a Service's lifecycle.
public enum ServiceEvent: Sendable, Hashable {
    case serviceStateChanged(ServiceStatus)
    case runStarted(Run)
    case runEnded(Run)

    public var serviceID: UUID {
        switch self {
        case .serviceStateChanged(let status): status.serviceID
        case .runStarted(let run), .runEnded(let run): run.serviceID
        }
    }
}
