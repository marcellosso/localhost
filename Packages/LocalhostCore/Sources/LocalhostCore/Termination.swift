import Darwin

/// How the command's shell ended.
public enum Termination: Sendable, Hashable, Codable {
    case exited(code: Int32)
    case signaled(Int32)

    /// Decodes a `waitpid` status.
    init(waitStatus status: Int32) {
        let signal = status & 0x7f
        self = signal == 0 ? .exited(code: (status >> 8) & 0xff) : .signaled(signal)
    }

    /// The launch failure this termination stands for, by shell convention.
    public var launchError: LocalhostError? {
        switch self {
        case .exited(code: 127): .commandNotFound
        case .exited(code: 126): .commandNotExecutable
        default: nil
        }
    }
}

/// How a process tree ended: the shell's termination, and how hard localhost
/// had to push to end the whole group.
public struct ProcessTreeExit: Sendable, Hashable {
    public let termination: Termination
    /// The last signal localhost sent to the process group (`SIGINT`,
    /// `SIGTERM` or `SIGKILL`), or `nil` if the tree ended on its own.
    public let strongestSignalSent: Int32?

    public var wasForceKilled: Bool { strongestSignalSent == SIGKILL }
}

/// How long to wait after each signal before escalating to the next.
public struct StopPolicy: Sendable, Hashable {
    public var interruptTimeout: Duration
    public var terminateTimeout: Duration

    public init(interruptTimeout: Duration, terminateTimeout: Duration) {
        self.interruptTimeout = interruptTimeout
        self.terminateTimeout = terminateTimeout
    }

    /// SIGINT → 5s → SIGTERM → 3s → SIGKILL.
    public static let `default` = StopPolicy(interruptTimeout: .seconds(5), terminateTimeout: .seconds(3))
}
