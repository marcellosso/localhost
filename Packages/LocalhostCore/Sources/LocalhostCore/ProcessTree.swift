import Darwin
import Foundation
import Synchronization

/// The process tree of one Run: `<shell> -l -c <commandLine>` launched in its
/// own session, so the shell's pid is also the id of the process group that
/// holds everything it spawns (ADR-0003).
///
/// The tree has ended once the shell has exited *and* no process is left in
/// its group. Processes still in the group when the shell exits on its own are
/// stopped (starting at SIGTERM: background jobs of a non-interactive shell
/// ignore SIGINT), so a Run never leaves anything behind.
public actor ProcessTree {
    /// The shell's pid, which is also the process group and session id.
    public nonisolated let pid: pid_t
    /// Every output line, in arrival order, in batches as read from the
    /// pipes (so chatty commands cost one element per read, not per line).
    /// Reading pauses while batches aren't taken. Finishes after the tree has ended.
    public nonisolated let output: OutputBatches

    private let stopPolicy: StopPolicy
    private let readers: [PipeReader]
    private let outputEnded = Latch()
    private var shellTermination: Termination?
    private var shellExitWaiters: [CheckedContinuation<Termination, Never>] = []
    private var isEscalating = false
    private var strongestSignalSent: Int32?
    private var exit: ProcessTreeExit?
    private var exitWaiters: [CheckedContinuation<ProcessTreeExit, Never>] = []

    /// How often to check whether the process group is gone.
    private static let pollInterval = Duration.milliseconds(20)
    /// How long to wait for the pipes to drain once the group is gone. A
    /// process that left the group may still hold them open.
    private static let outputDrainTimeout = Duration.milliseconds(500)

    /// Launches the command line. Fails before anything runs if the working
    /// directory or the shell is missing.
    public static func launch(
        _ configuration: LaunchConfiguration,
        stopPolicy: StopPolicy = .default
    ) throws(LocalhostError) -> ProcessTree {
        var isDirectory: ObjCBool = false
        let directory = configuration.workingDirectory.path
        guard FileManager.default.fileExists(atPath: directory, isDirectory: &isDirectory), isDirectory.boolValue else {
            throw .workingDirectoryMissing(directory)
        }
        guard FileManager.default.fileExists(atPath: configuration.shell) else {
            throw .shellNotFound(configuration.shell)
        }

        let stdout = try Pipe.make()
        let stderr = try Pipe.make()
        let pid: pid_t
        do {
            pid = try spawn(configuration, stdout: stdout.write, stderr: stderr.write)
        } catch {
            for fd in [stdout.read, stdout.write, stderr.read, stderr.write] { close(fd) }
            throw error
        }
        close(stdout.write)
        close(stderr.write)

        let tree = ProcessTree(pid: pid, stdout: stdout.read, stderr: stderr.read, stopPolicy: stopPolicy)
        Task { await tree.monitor() }
        return tree
    }

    private init(pid: pid_t, stdout: Int32, stderr: Int32, stopPolicy: StopPolicy) {
        self.pid = pid
        self.stopPolicy = stopPolicy
        let (output, continuation) = AsyncStream.makeStream(of: [OutputLine].self)
        let flow = OutputFlow()
        self.output = OutputBatches(stream: output, flow: flow)

        // The stream finishes when both pipes have ended.
        let openPipes = Mutex(2)
        let outputEnded = outputEnded
        let reportEnd: @Sendable () -> Void = {
            let remaining = openPipes.withLock { count in
                count -= 1
                return count
            }
            guard remaining == 0 else { return }
            continuation.finish()
            outputEnded.open()
        }
        let yield: @Sendable ([OutputLine]) -> Void = { continuation.yield($0) }
        readers = [
            PipeReader(fileDescriptor: stdout, channel: .stdout, flow: flow, onLines: yield, onEnd: reportEnd),
            PipeReader(fileDescriptor: stderr, channel: .stderr, flow: flow, onLines: yield, onEnd: reportEnd),
        ]
    }

    /// Waits until the shell has exited. Processes it left in the group may
    /// still be running; `waitForExit()` waits for those too.
    public func waitForShellExit() async -> Termination {
        if let shellTermination { return shellTermination }
        return await withCheckedContinuation { shellExitWaiters.append($0) }
    }

    /// Waits until the tree has ended.
    public func waitForExit() async -> ProcessTreeExit {
        if let exit { return exit }
        return await withCheckedContinuation { exitWaiters.append($0) }
    }

    /// Stops the whole process group: SIGINT, then SIGTERM, then SIGKILL, each
    /// sent only if the group outlives the previous one's timeout. Returns
    /// once the tree has ended. Safe to call repeatedly and concurrently.
    @discardableResult
    public func stop() async -> ProcessTreeExit {
        if let exit { return exit }
        await escalate()
        return await waitForExit()
    }

    // MARK: Private

    private func monitor() async {
        let status = await Self.waitForShell(pid)
        let termination = Termination(waitStatus: status)
        shellTermination = termination
        for waiter in shellExitWaiters { waiter.resume(returning: termination) }
        shellExitWaiters = []

        // Anything still in the group is left over from the shell: stop it.
        if processGroupExists() { await escalate(from: SIGTERM) }
        while processGroupExists() { try? await Task.sleep(for: Self.pollInterval) }
        await waitForOutputToEnd(timeout: Self.outputDrainTimeout)

        let exit = ProcessTreeExit(termination: termination, strongestSignalSent: strongestSignalSent)
        self.exit = exit
        for waiter in exitWaiters { waiter.resume(returning: exit) }
        exitWaiters = []
    }

    /// Sends the escalating signals, beginning with `firstSignal`, until the
    /// group is gone. Only one escalation runs at a time; later callers return
    /// immediately.
    private func escalate(from firstSignal: Int32 = SIGINT) async {
        guard !isEscalating else { return }
        isEscalating = true
        let steps: [(Int32, Duration?)] = [
            (SIGINT, stopPolicy.interruptTimeout),
            (SIGTERM, stopPolicy.terminateTimeout),
            (SIGKILL, nil),
        ]
        for (signal, timeout) in steps.drop(while: { $0.0 != firstSignal }) {
            guard processGroupExists() else { return }
            strongestSignalSent = signal
            kill(-pid, signal)
            if await waitForProcessGroupToEnd(timeout: timeout) { return }
        }
    }

    /// Returns whether the group ended within `timeout` (`nil` waits forever).
    private func waitForProcessGroupToEnd(timeout: Duration?) async -> Bool {
        let deadline = timeout.map { ContinuousClock.now + $0 }
        while processGroupExists() {
            if let deadline, ContinuousClock.now >= deadline { return false }
            try? await Task.sleep(for: Self.pollInterval)
        }
        return true
    }

    private func processGroupExists() -> Bool {
        kill(-pid, 0) == 0 || errno == EPERM
    }

    /// Waits for both pipes to end, cancelling the readers after `timeout`.
    private func waitForOutputToEnd(timeout: Duration) async {
        let timer = Task { [readers] in
            try await Task.sleep(for: timeout)
            for reader in readers { reader.cancel() }
        }
        await outputEnded.wait()
        timer.cancel()
    }

    /// Blocks a dispatch thread in `waitpid` until the shell exits, and reaps it.
    private static func waitForShell(_ pid: pid_t) async -> Int32 {
        await withCheckedContinuation { continuation in
            DispatchQueue.global().async {
                var status: Int32 = 0
                while waitpid(pid, &status, 0) == -1, errno == EINTR {}
                continuation.resume(returning: status)
            }
        }
    }

    /// `posix_spawn`s the shell in a new session with stdin on /dev/null,
    /// stdout/stderr on the given pipes, default signal handling, and no other
    /// inherited file descriptors.
    private static func spawn(
        _ configuration: LaunchConfiguration,
        stdout: Int32,
        stderr: Int32
    ) throws(LocalhostError) -> pid_t {
        var attributes: posix_spawnattr_t?
        posix_spawnattr_init(&attributes)
        defer { posix_spawnattr_destroy(&attributes) }
        let flags = POSIX_SPAWN_SETSID | POSIX_SPAWN_CLOEXEC_DEFAULT | POSIX_SPAWN_SETSIGDEF | POSIX_SPAWN_SETSIGMASK
        posix_spawnattr_setflags(&attributes, Int16(flags))
        var allSignals = ~sigset_t(0)
        var noSignals = sigset_t(0)
        posix_spawnattr_setsigdefault(&attributes, &allSignals)
        posix_spawnattr_setsigmask(&attributes, &noSignals)

        var fileActions: posix_spawn_file_actions_t?
        posix_spawn_file_actions_init(&fileActions)
        defer { posix_spawn_file_actions_destroy(&fileActions) }
        posix_spawn_file_actions_addopen(&fileActions, STDIN_FILENO, "/dev/null", O_RDONLY, 0)
        posix_spawn_file_actions_adddup2(&fileActions, stdout, STDOUT_FILENO)
        posix_spawn_file_actions_adddup2(&fileActions, stderr, STDERR_FILENO)
        posix_spawn_file_actions_addchdir_np(&fileActions, configuration.workingDirectory.path)

        let arguments = [configuration.shell, "-l", "-c", configuration.commandLine]
        let environment = configuration.environment.map { "\($0.key)=\($0.value)" }
        var pid: pid_t = 0
        let result = withCStringArray(arguments) { argv in
            withCStringArray(environment) { envp in
                posix_spawn(&pid, configuration.shell, &fileActions, &attributes, argv, envp)
            }
        }
        guard result == 0 else {
            throw result == ENOENT ? .shellNotFound(configuration.shell) : .launchFailed(errno: result)
        }
        return pid
    }
}

/// A one-shot signal: `wait()` returns once `open()` has been called.
private final class Latch: Sendable {
    private let state = Mutex<(isOpen: Bool, waiters: [CheckedContinuation<Void, Never>])>((false, []))

    func open() {
        let waiters = state.withLock { state in
            state.isOpen = true
            defer { state.waiters = [] }
            return state.waiters
        }
        for waiter in waiters { waiter.resume() }
    }

    func wait() async {
        await withCheckedContinuation { continuation in
            let isOpen = state.withLock { state in
                if !state.isOpen { state.waiters.append(continuation) }
                return state.isOpen
            }
            if isOpen { continuation.resume() }
        }
    }
}

private struct Pipe {
    let read: Int32
    let write: Int32

    /// A pipe whose ends are close-on-exec, so no other spawned process inherits them.
    static func make() throws(LocalhostError) -> Pipe {
        var fds: [Int32] = [0, 0]
        guard pipe(&fds) == 0 else { throw .launchFailed(errno: errno) }
        for fd in fds { _ = fcntl(fd, F_SETFD, FD_CLOEXEC) }
        return Pipe(read: fds[0], write: fds[1])
    }
}

/// Calls `body` with a NULL-terminated C array of the strings.
private func withCStringArray<R>(_ strings: [String], _ body: ([UnsafeMutablePointer<CChar>?]) -> R) -> R {
    let pointers = strings.map { strdup($0) } + [nil]
    defer { for pointer in pointers { free(pointer) } }
    return body(pointers)
}
