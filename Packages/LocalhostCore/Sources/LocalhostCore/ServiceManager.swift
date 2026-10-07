import Foundation

/// Runs Services: owns each Service's state and its at most one active Run.
///
/// Start, Stop and Restart are safe to call in any state and from anywhere:
/// operations on one Service run one at a time, in call order, and each
/// returns the Service's status once it has taken effect.
public actor ServiceManager {
    private let library: ProjectLibrary
    private let logs: LogStore
    private let runRegistry: RunRegistry
    private let baseEnvironment: [String: String]
    private let gracePeriod: Duration
    private let stopPolicy: StopPolicy

    private var statuses: [UUID: ServiceStatus] = [:]
    /// The process tree of each Service's active Run. It outlives the Service's
    /// Starting/Running states while leftovers are being cleaned up.
    private var trees: [UUID: ProcessTree] = [:]
    /// The last queued operation of each Service.
    private var operations: [UUID: Task<Void, Never>] = [:]
    private var subscribers: [UUID: AsyncStream<ServiceEvent>.Continuation] = [:]

    /// - Parameters:
    ///   - baseEnvironment: The environment every Run starts from (see `RunEnvironment`).
    ///   - gracePeriod: How long a Run must stay alive in Starting to be Running.
    public init(
        library: ProjectLibrary,
        logs: LogStore,
        runRegistry: RunRegistry,
        baseEnvironment: [String: String] = ProcessInfo.processInfo.environment,
        gracePeriod: Duration = .milliseconds(1500),
        stopPolicy: StopPolicy = .default
    ) {
        self.library = library
        self.logs = logs
        self.runRegistry = runRegistry
        self.baseEnvironment = baseEnvironment
        self.gracePeriod = gracePeriod
        self.stopPolicy = stopPolicy
    }

    // MARK: Observing

    public func status(of serviceID: UUID) -> ServiceStatus {
        statuses[serviceID] ?? .neverRun(serviceID)
    }

    /// The status of every Service that has run, plus a stream of every change
    /// after that snapshot. Taking both at once means no change falls between them.
    public func subscribe() -> (snapshot: [UUID: ServiceStatus], events: AsyncStream<ServiceEvent>) {
        let (stream, continuation) = AsyncStream.makeStream(of: ServiceEvent.self)
        let id = UUID()
        subscribers[id] = continuation
        continuation.onTermination = { [weak self] _ in
            Task { await self?.unsubscribe(id) }
        }
        return (statuses, stream)
    }

    // MARK: Lifecycle

    /// Starts a Run unless one is Starting or Running. If the Service is
    /// Stopping, waits for that to finish first. A Run that cannot launch
    /// leaves the Service Failed, with the reason in `run.failure`.
    @discardableResult
    public func start(_ serviceID: UUID, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        _ = try await library.service(id: serviceID)
        return await serialized(serviceID) { await self.performStart(serviceID, origin: origin) }
    }

    /// Stops the active Run, if any, and returns once its whole process tree
    /// has ended. A requested stop always ends Stopped.
    @discardableResult
    public func stop(_ serviceID: UUID, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        _ = try await library.service(id: serviceID)
        return await serialized(serviceID) { await self.performStop(serviceID, origin: origin) }
    }

    /// Stops the active Run, if any, then starts a new one.
    @discardableResult
    public func restart(_ serviceID: UUID, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        _ = try await library.service(id: serviceID)
        return await serialized(serviceID) {
            _ = await self.performStop(serviceID, origin: origin)
            return await self.performStart(serviceID, origin: origin)
        }
    }

    // MARK: Operations

    private func performStart(_ serviceID: UUID, origin: Origin) async -> ServiceStatus {
        if status(of: serviceID).state == .starting || status(of: serviceID).state == .running {
            return status(of: serviceID)
        }
        // A Run that ended on its own may still be stopping its leftovers.
        await waitForActiveRunToEnd(serviceID)

        var run = Run(id: UUID(), serviceID: serviceID, origin: origin, startedAt: Date())
        let tree: ProcessTree
        do {
            let (project, service) = try await library.service(id: serviceID)
            tree = try ProcessTree.launch(
                LaunchConfiguration(
                    commandLine: service.commandLine,
                    workingDirectory: project.workingDirectoryURL(for: service),
                    environment: RunEnvironment.merged(base: baseEnvironment, service: service.environment)
                ),
                stopPolicy: stopPolicy
            )
        } catch {
            run.failure = error
            run.endedAt = Date()
            broadcast(.runStarted(run))
            update(ServiceStatus(serviceID: serviceID, state: .failed, run: run))
            broadcast(.runEnded(run))
            return status(of: serviceID)
        }

        trees[serviceID] = tree
        if let startTime = ProcessStartTime.of(tree.pid) {
            runRegistry.record(run, pid: tree.pid, startTime: startTime)
        }
        await logs.beginRun(run)
        broadcast(.runStarted(run))
        update(ServiceStatus(serviceID: serviceID, state: .starting, run: run))

        Task { [logs, runID = run.id] in
            for await line in tree.output {
                await logs.append(line, toRun: runID)
            }
            let exit = await tree.waitForExit()
            for note in Self.logNotes(for: exit) {
                await logs.append(OutputLine(channel: .localhost, text: note, timestamp: Date()), toRun: runID)
            }
            await logs.endRun(runID)
        }
        Task { [gracePeriod, runID = run.id] in
            try? await Task.sleep(for: gracePeriod)
            self.gracePeriodElapsed(serviceID, runID: runID)
        }
        Task { [runID = run.id] in
            let termination = await tree.waitForShellExit()
            self.shellExited(serviceID, runID: runID, termination: termination)
            let exit = await tree.waitForExit()
            self.runEnded(serviceID, runID: runID, exit: exit)
        }
        return status(of: serviceID)
    }

    private func performStop(_ serviceID: UUID, origin: Origin) async -> ServiceStatus {
        var status = status(of: serviceID)
        guard status.state == .starting || status.state == .running,
              let tree = trees[serviceID], let runID = status.run?.id
        else { return status }

        status.state = .stopping
        status.run?.stopOrigin = origin
        update(status)
        let exit = await tree.stop()
        runEnded(serviceID, runID: runID, exit: exit)
        return self.status(of: serviceID)
    }

    private func waitForActiveRunToEnd(_ serviceID: UUID) async {
        guard let tree = trees[serviceID], let runID = statuses[serviceID]?.run?.id else { return }
        let exit = await tree.waitForExit()
        runEnded(serviceID, runID: runID, exit: exit)
    }

    // MARK: Transitions

    private func gracePeriodElapsed(_ serviceID: UUID, runID: UUID) {
        var status = status(of: serviceID)
        guard status.run?.id == runID, status.state == .starting else { return }
        status.state = .running
        update(status)
    }

    /// The shell exited: the Service is no longer up, even if leftovers in its
    /// process group are still being stopped.
    private func shellExited(_ serviceID: UUID, runID: UUID, termination: Termination) {
        var status = status(of: serviceID)
        guard status.run?.id == runID, status.run?.endedAt == nil else { return }
        status.run?.termination = termination
        status.run?.failure = termination.launchError
        status.state = Self.stateAfterEnding(status.state, termination: termination)
        update(status)
    }

    /// The whole process tree has ended. Safe to call more than once per Run.
    private func runEnded(_ serviceID: UUID, runID: UUID, exit: ProcessTreeExit) {
        var status = status(of: serviceID)
        guard var run = status.run, run.id == runID, run.endedAt == nil else { return }
        run.endedAt = Date()
        run.termination = exit.termination
        run.failure = exit.termination.launchError
        run.wasForceKilled = exit.wasForceKilled
        status.run = run
        status.state = Self.stateAfterEnding(status.state, termination: exit.termination)
        trees[serviceID] = nil
        runRegistry.remove(runID)
        update(status)
        broadcast(.runEnded(run))
    }

    /// What localhost writes into a Run's log once it has ended.
    private static func logNotes(for exit: ProcessTreeExit) -> [String] {
        var notes: [String] = []
        if exit.wasForceKilled {
            notes.append("Force-killed: the process group did not stop in time")
        }
        notes.append(exit.termination.summary)
        return notes
    }

    /// The state a Service moves to when its command ends while in `state`.
    private static func stateAfterEnding(_ state: ServiceState, termination: Termination) -> ServiceState {
        switch state {
        case .stopping: .stopped
        case .starting: .failed
        case .running: termination == .exited(code: 0) ? .exited : .failed
        case .stopped, .exited, .failed: state
        }
    }

    // MARK: Plumbing

    /// Stores `status`, broadcasting `serviceStateChanged` if the state changed.
    private func update(_ status: ServiceStatus) {
        let previousState = statuses[status.serviceID]?.state
        statuses[status.serviceID] = status
        if status.state != previousState {
            broadcast(.serviceStateChanged(status))
        }
    }

    private func serialized<T: Sendable>(
        _ serviceID: UUID,
        _ operation: @escaping @Sendable () async -> T
    ) async -> T {
        let previous = operations[serviceID]
        let task = Task {
            await previous?.value
            return await operation()
        }
        operations[serviceID] = Task { _ = await task.value }
        return await task.value
    }

    private func broadcast(_ event: ServiceEvent) {
        for continuation in subscribers.values {
            continuation.yield(event)
        }
    }

    private func unsubscribe(_ id: UUID) {
        subscribers[id] = nil
    }
}
