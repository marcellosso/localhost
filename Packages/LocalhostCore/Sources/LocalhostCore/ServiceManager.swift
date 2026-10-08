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
    private let portTimeout: Duration
    private let portProbeInterval: Duration
    private let stopPolicy: StopPolicy
    /// Whether a Run may move to a free port when its Service's Port is taken.
    private var isAutomaticPortMoveEnabled: Bool

    private var statuses: [UUID: ServiceStatus] = [:]
    /// The process tree of each Service's active Run. It outlives the Service's
    /// Starting/Running states while leftovers are being cleaned up.
    private var trees: [UUID: ProcessTree] = [:]
    /// The last queued operation of each Service.
    private var operations: [UUID: Task<Void, Never>] = [:]
    private var subscribers: [UUID: AsyncStream<ServiceEvent>.Continuation] = [:]
    /// Services that are Starting but still waiting for their Dependencies,
    /// with a token identifying that start request.
    private var pendingStarts: [UUID: UUID] = [:]
    /// Waiting for the next status update of a Service.
    private var updateWaiters: [UUID: [CheckedContinuation<Void, Never>]] = [:]
    /// The port of each Service's active Run, from launch until its tree has ended.
    private var runPorts: [UUID: Int] = [:]

    /// - Parameters:
    ///   - baseEnvironment: The environment every Run starts from (see `RunEnvironment`).
    ///   - gracePeriod: How long a Run without a Port must stay alive in Starting to be Running.
    ///   - portTimeout: How long a Run with a Port waits in Starting for the
    ///     Port to accept connections before it is Running anyway, with a warning.
    ///   - portProbeInterval: How often the Port is checked meanwhile.
    ///   - isAutomaticPortMoveEnabled: Whether a Run may move to a free port
    ///     when its Service's Port is taken (see `setAutomaticPortMove`).
    public init(
        library: ProjectLibrary,
        logs: LogStore,
        runRegistry: RunRegistry,
        baseEnvironment: [String: String] = ProcessInfo.processInfo.environment,
        gracePeriod: Duration = .milliseconds(1500),
        portTimeout: Duration = .seconds(30),
        portProbeInterval: Duration = .milliseconds(250),
        stopPolicy: StopPolicy = .default,
        isAutomaticPortMoveEnabled: Bool = true
    ) {
        self.library = library
        self.logs = logs
        self.runRegistry = runRegistry
        self.baseEnvironment = baseEnvironment
        self.gracePeriod = gracePeriod
        self.portTimeout = portTimeout
        self.portProbeInterval = portProbeInterval
        self.stopPolicy = stopPolicy
        self.isAutomaticPortMoveEnabled = isAutomaticPortMoveEnabled
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
    ///
    /// Dependencies that are not Running or Exited are started first, and the
    /// Service stays Starting until they are. If one Fails, the Service Fails
    /// with `dependencyFailed`; if one is stopped meanwhile, it ends Stopped.
    ///
    /// A Service with a Port whose Port is taken moves to the next free port
    /// when allowed (see `nextFreePort(for:)`), else Fails with `portConflict`.
    ///
    /// - Parameter port: Start on this port instead of the Service's Port, for
    ///   this Run only. Ignored for a Service without a Port.
    @discardableResult
    public func start(_ serviceID: UUID, onPort port: Int? = nil, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        let (project, service) = try await library.service(id: serviceID)
        if let port, !(1...65_535).contains(port) { throw .invalidPort(port) }
        if service.dependencies.isEmpty {
            return await serialized(serviceID) { await self.performStart(serviceID, requestedPort: port, origin: origin) }
        }
        await startWithDependencies(serviceID, in: project, requestedPort: port, origin: origin)
        return status(of: serviceID)
    }

    /// Stops the active Run, if any, and returns once its whole process tree
    /// has ended. A requested stop always ends Stopped, including a start
    /// still waiting for its Dependencies.
    @discardableResult
    public func stop(_ serviceID: UUID, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        _ = try await library.service(id: serviceID)
        if cancelPendingStart(serviceID) { return status(of: serviceID) }
        return await serialized(serviceID) { await self.performStop(serviceID, origin: origin) }
    }

    /// Stops the active Run, if any, then starts a new one (with its
    /// Dependencies, like `start`). Dependents are left alone.
    @discardableResult
    public func restart(_ serviceID: UUID, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        let (_, service) = try await library.service(id: serviceID)
        if service.dependencies.isEmpty && pendingStarts[serviceID] == nil {
            return await serialized(serviceID) {
                _ = await self.performStop(serviceID, origin: origin)
                return await self.performStart(serviceID, requestedPort: nil, origin: origin)
            }
        }
        try await stop(serviceID, origin: origin)
        return try await start(serviceID, origin: origin)
    }

    /// Starts every Service of the Project except those excluded from Start
    /// All, each after its Dependencies. Returns once every start has taken
    /// effect (launched, Failed, or cancelled).
    public func startAll(inProject projectID: UUID, origin: Origin) async throws(LocalhostError) {
        let project = try await library.project(id: projectID)
        let serviceIDs = project.services.filter { !$0.isExcludedFromStartAll }.map(\.id)
        // Mark every Service Starting before any of them checks its
        // Dependencies, so none mistakes an earlier Run's Exited for this one.
        let begun = serviceIDs.compactMap { id in beginPendingStart(id).map { (id, $0) } }
        await withDiscardingTaskGroup { group in
            for (serviceID, token) in begun {
                group.addTask { await self.completePendingStart(serviceID, token: token, in: project, origin: origin) }
            }
            for serviceID in serviceIDs where !begun.contains(where: { $0.0 == serviceID }) {
                group.addTask { await self.startWithDependencies(serviceID, in: project, origin: origin) }
            }
        }
    }

    /// Stops every Service of the Project, each after the Services that
    /// depend on it. Starts still waiting for their Dependencies are cancelled.
    public func stopAll(inProject projectID: UUID, origin: Origin) async throws(LocalhostError) {
        let project = try await library.project(id: projectID)
        for service in project.services {
            cancelPendingStart(service.id)
        }
        var stops: [UUID: Task<Void, Never>] = [:]
        for serviceID in project.dependencyOrder(of: project.services.map(\.id)).reversed() {
            let dependentStops = project.dependents(of: serviceID).compactMap { stops[$0.id] }
            stops[serviceID] = Task {
                for stop in dependentStops { await stop.value }
                _ = await self.serialized(serviceID) { await self.performStop(serviceID, origin: origin) }
            }
        }
        for stop in stops.values { await stop.value }
    }

    /// Stops every live process tree in parallel, e.g. before quitting.
    /// Returns once all of them have ended.
    public func stopAll(origin: Origin) async {
        for serviceID in Array(pendingStarts.keys) {
            cancelPendingStart(serviceID)
        }
        let serviceIDs = Array(trees.keys)
        await withDiscardingTaskGroup { group in
            for serviceID in serviceIDs {
                group.addTask {
                    _ = await self.serialized(serviceID) { await self.performStopForQuit(serviceID, origin: origin) }
                }
            }
        }
    }

    // MARK: Listeners

    /// Turns automatic port moves on or off (an app setting, on by default).
    public func setAutomaticPortMove(_ isEnabled: Bool) {
        isAutomaticPortMoveEnabled = isEnabled
    }

    /// Every Listener on this machine, by first port, attributed to the Run
    /// whose process tree it is in, else to the Project whose folder holds its
    /// working directory (the deepest one), else to no one.
    public func listeners() async -> [Listener] {
        async let launchdJobs = LaunchdJobs.running()
        let processes = await ListenerScanner.scan()
        let projectFolders = await library.projects().map { project in
            // The kernel reports working directories with symlinks resolved.
            (project.id, ListenerScanner.resolvedPath(project.path))
        }
        let jobs = await launchdJobs
        // Each Run's shell leads its own session and process group (ADR-0003).
        var runsBySession: [pid_t: (serviceID: UUID, runID: UUID)] = [:]
        for (serviceID, tree) in trees {
            if let runID = statuses[serviceID]?.run?.id { runsBySession[tree.pid] = (serviceID, runID) }
        }
        return processes.map { process in
            let owner: ListenerOwner
            if let run = runsBySession[process.sessionID] ?? runsBySession[process.processGroupID] {
                owner = .run(serviceID: run.serviceID, runID: run.runID)
            } else if let directory = process.workingDirectory,
                      let project = projectFolders
                          .filter({ directory == $0.1 || directory.hasPrefix($0.1.hasSuffix("/") ? $0.1 : $0.1 + "/") })
                          .max(by: { $0.1.count < $1.1.count }) {
                owner = .project(project.0)
            } else {
                owner = .unmanaged
            }
            return Listener(
                pid: process.pid, processGroupID: process.processGroupID, name: process.name,
                command: process.command, ports: process.ports, workingDirectory: process.workingDirectory,
                owner: owner, launchdLabel: jobs[process.pid] ?? jobs[process.processGroupID],
                startTime: process.startTime
            )
        }
        .sorted { ($0.ports.first ?? 0, $0.pid) < ($1.ports.first ?? 0, $1.pid) }
    }

    /// Stops a Listener and returns once it has ended. One that belongs to a
    /// Run stops that Service. Any other gets SIGINT → SIGTERM → SIGKILL on
    /// its process group (or, if the group is led by a session leader such as
    /// a terminal's shell, on its process and descendants). localhost itself
    /// and launchd jobs are refused.
    public func stopListener(_ listener: Listener, origin: Origin) async throws(LocalhostError) {
        if case .run(let serviceID, let runID) = listener.owner {
            guard statuses[serviceID]?.run?.id == runID else { return }
            try await stop(serviceID, origin: origin)
            return
        }
        guard listener.pid != getpid() else { throw .listenerIsLocalhost }
        if let label = listener.launchdLabel { throw .listenerIsLaunchdJob(label: label) }
        await ListenerStopper.stop(listener, policy: stopPolicy)
    }

    /// The port a start of the Service would move to if its Port were taken:
    /// the first free port above it, up to +100, that is not another
    /// Service's Port. `nil` if the Service has no Port or none is free.
    public func nextFreePort(for serviceID: UUID) async throws(LocalhostError) -> Int? {
        let (_, service) = try await library.service(id: serviceID)
        guard let port = service.port else { return nil }
        return await nextFreePort(above: port, for: serviceID, listeners: listeners())
    }

    /// The port a Run of `service` gets, and why it is not the Port if it isn't.
    private func portForRun(
        of service: Service, configuredPort: Int, in project: Project, requestedPort: Int?
    ) async throws(LocalhostError) -> (Int, PortChange?) {
        let listeners = await listeners()
        if let requestedPort {
            let (isTaken, holder) = await holder(of: requestedPort, for: service.id, listeners: listeners)
            if isTaken { throw .portConflict(port: requestedPort, owner: holder) }
            let change = requestedPort == configuredPort ? nil : PortChange(configuredPort: configuredPort, reason: .requested)
            return (requestedPort, change)
        }
        let (isTaken, holder) = await holder(of: configuredPort, for: service.id, listeners: listeners)
        guard isTaken else { return (configuredPort, nil) }
        guard canMoveAutomatically(service, in: project, awayFrom: holder),
              let freePort = await nextFreePort(above: configuredPort, for: service.id, listeners: listeners)
        else { throw .portConflict(port: configuredPort, owner: holder) }
        return (freePort, PortChange(configuredPort: configuredPort, reason: .portInUse(holder: holder)))
    }

    /// Whether `port` is taken, and by which Listener if localhost can see it.
    private func holder(of port: Int, for serviceID: UUID, listeners: [Listener]) async -> (isTaken: Bool, holder: Listener?) {
        if let listener = listeners.first(where: { $0.ports.contains(port) }) { return (true, listener) }
        // Another Run launched on it that isn't listening yet.
        if runPorts.contains(where: { $0.key != serviceID && $0.value == port }) { return (true, nil) }
        return (await PortProbe.isAccepting(port), nil)
    }

    /// Never when it is off, for this Service or app-wide; when dependents may
    /// expect the Port (#15); or when the holder is inside the same Project,
    /// likely its own leftover.
    private func canMoveAutomatically(_ service: Service, in project: Project, awayFrom holder: Listener?) -> Bool {
        guard isAutomaticPortMoveEnabled, service.portCanChange, project.dependents(of: service.id).isEmpty else { return false }
        switch holder?.owner {
        case .project(let projectID): return projectID != project.id
        case .run(let holderServiceID, _): return project.service(id: holderServiceID) == nil
        case .unmanaged, nil: return true
        }
    }

    private func nextFreePort(above port: Int, for serviceID: UUID, listeners: [Listener]) async -> Int? {
        guard port < 65_535 else { return nil }
        let otherServicePorts = await library.projects().flatMap(\.services).filter { $0.id != serviceID }.compactMap(\.port)
        let taken = Set(otherServicePorts)
            .union(listeners.flatMap(\.ports))
            .union(runPorts.filter { $0.key != serviceID }.values)
        for candidate in (port + 1)...min(port + 100, 65_535) where !taken.contains(candidate) {
            if await !PortProbe.isAccepting(candidate) { return candidate }
        }
        return nil
    }

    // MARK: Dependencies

    private enum DependencyOutcome: Sendable {
        case satisfied
        case failed(Key)
        case stopped
    }

    /// `start` for a Service with Dependencies.
    private func startWithDependencies(_ serviceID: UUID, in project: Project, requestedPort: Int? = nil, origin: Origin) async {
        while status(of: serviceID).state == .stopping {
            await nextUpdate(of: serviceID)
        }
        guard let token = beginPendingStart(serviceID) else { return }
        await completePendingStart(serviceID, token: token, in: project, requestedPort: requestedPort, origin: origin)
    }

    /// Marks the Service Starting, waiting for its Dependencies. Returns `nil`
    /// if it is already Starting, Running or Stopping.
    private func beginPendingStart(_ serviceID: UUID) -> UUID? {
        var status = status(of: serviceID)
        guard !status.state.isActive, pendingStarts[serviceID] == nil else { return nil }
        let token = UUID()
        pendingStarts[serviceID] = token
        status.state = .starting
        update(status)
        return token
    }

    /// Waits for the Dependencies of a pending start, then launches its Run
    /// unless the start was cancelled meanwhile.
    private func completePendingStart(
        _ serviceID: UUID, token: UUID, in project: Project, requestedPort: Int? = nil, origin: Origin
    ) async {
        let dependencies = project.service(id: serviceID)?.dependencies ?? []
        setAwaitedDependencies(dependencies, of: serviceID, token: token)
        let outcomes = await withTaskGroup(of: DependencyOutcome.self) { group in
            for dependency in dependencies {
                group.addTask {
                    let outcome = await self.awaitDependency(dependency, in: project, origin: origin)
                    await self.dependencySettled(dependency, of: serviceID, token: token)
                    return outcome
                }
            }
            var outcomes: [DependencyOutcome] = []
            for await outcome in group { outcomes.append(outcome) }
            return outcomes
        }
        guard pendingStarts[serviceID] == token else { return }

        if let failed = outcomes.lazy.compactMap({ if case .failed(let key) = $0 { key } else { nil } }).first {
            pendingStarts[serviceID] = nil
            recordFailedLaunch(serviceID, origin: origin, failure: .dependencyFailed(failed))
        } else if outcomes.contains(where: { if case .stopped = $0 { true } else { false } }) {
            cancelPendingStart(serviceID)
        } else {
            _ = await serialized(serviceID) {
                await self.launchPendingStart(serviceID, token: token, requestedPort: requestedPort, origin: origin)
            }
        }
    }

    private func launchPendingStart(_ serviceID: UUID, token: UUID, requestedPort: Int?, origin: Origin) async -> ServiceStatus {
        // Leftovers of the previous Run end while the Service is still pending.
        await waitForActiveRunToEnd(serviceID)
        guard pendingStarts[serviceID] == token else { return status(of: serviceID) }
        pendingStarts[serviceID] = nil
        return await launch(serviceID, requestedPort: requestedPort, origin: origin)
    }

    /// Waits until a Dependency is Running or Exited, starting it if needed.
    private func awaitDependency(_ serviceID: UUID, in project: Project, origin: Origin) async -> DependencyOutcome {
        // A Dependency removed from the Project no longer holds anything up.
        guard let key = project.service(id: serviceID)?.key else { return .satisfied }
        // Once the Dependency is (being) started for us, its ending Failed or
        // Stopped is final; before that, it is a state to start it from.
        var isEngaged = false
        while true {
            switch status(of: serviceID).state {
            case .running, .exited:
                return .satisfied
            case .starting:
                isEngaged = true
                await nextUpdate(of: serviceID)
            case .stopping:
                await nextUpdate(of: serviceID)
            case .failed where isEngaged:
                return .failed(key)
            case .stopped where isEngaged:
                return .stopped
            case .stopped, .failed:
                isEngaged = true
                if let token = beginPendingStart(serviceID) {
                    Task { await self.completePendingStart(serviceID, token: token, in: project, origin: origin) }
                }
            }
        }
    }

    private func setAwaitedDependencies(_ dependencies: [UUID], of serviceID: UUID, token: UUID) {
        guard pendingStarts[serviceID] == token else { return }
        var status = status(of: serviceID)
        status.awaitedDependencies = dependencies
        update(status)
    }

    private func dependencySettled(_ dependency: UUID, of serviceID: UUID, token: UUID) {
        guard pendingStarts[serviceID] == token else { return }
        var status = status(of: serviceID)
        status.awaitedDependencies.removeAll { $0 == dependency }
        update(status)
    }

    /// Ends a start that is still waiting for its Dependencies as Stopped.
    /// Returns whether there was one.
    @discardableResult
    private func cancelPendingStart(_ serviceID: UUID) -> Bool {
        guard pendingStarts.removeValue(forKey: serviceID) != nil else { return false }
        var status = status(of: serviceID)
        status.state = .stopped
        status.awaitedDependencies = []
        update(status)
        return true
    }

    private func nextUpdate(of serviceID: UUID) async {
        await withCheckedContinuation { updateWaiters[serviceID, default: []].append($0) }
    }

    // MARK: Operations

    private func performStart(_ serviceID: UUID, requestedPort: Int?, origin: Origin) async -> ServiceStatus {
        if status(of: serviceID).state == .starting || status(of: serviceID).state == .running {
            return status(of: serviceID)
        }
        return await launch(serviceID, requestedPort: requestedPort, origin: origin)
    }

    /// Launches a new Run once the previous one has fully ended.
    private func launch(_ serviceID: UUID, requestedPort: Int? = nil, origin: Origin) async -> ServiceStatus {
        // A Run that ended on its own may still be stopping its leftovers.
        await waitForActiveRunToEnd(serviceID)

        var run = Run(id: UUID(), serviceID: serviceID, origin: origin, startedAt: Date())
        let tree: ProcessTree
        do {
            let (project, service) = try await library.service(id: serviceID)
            if let port = service.port {
                (run.port, run.portChange) = try await portForRun(of: service, configuredPort: port, in: project, requestedPort: requestedPort)
            }
            tree = try ProcessTree.launch(
                LaunchConfiguration(
                    commandLine: service.commandLine,
                    workingDirectory: project.workingDirectoryURL(for: service),
                    environment: RunEnvironment.merged(
                        base: baseEnvironment, service: service.environment, port: run.port
                    )
                ),
                stopPolicy: stopPolicy
            )
        } catch {
            recordFailedLaunch(serviceID, origin: origin, failure: error)
            return status(of: serviceID)
        }

        trees[serviceID] = tree
        runPorts[serviceID] = run.port
        if let startTime = ProcessStartTime.of(tree.pid) {
            runRegistry.record(run, pid: tree.pid, startTime: startTime)
        }
        await logs.beginRun(run)
        if let port = run.port, let change = run.portChange {
            await logs.append(OutputLine(channel: .localhost, text: Self.logNote(forPort: port, change: change), timestamp: Date()), toRun: run.id)
        }
        broadcast(.runStarted(run))
        update(ServiceStatus(serviceID: serviceID, state: .starting, run: run))

        Task { [logs, runID = run.id] in
            var hasDetectedURL = false
            for await line in tree.output {
                await logs.append(line, toRun: runID)
                if !hasDetectedURL, line.channel != .localhost, let url = LocalURL.first(in: ANSI.strip(line.text)) {
                    hasDetectedURL = true
                    self.urlDetected(url, serviceID: serviceID, runID: runID)
                }
            }
            let exit = await tree.waitForExit()
            for note in Self.logNotes(for: exit) {
                await logs.append(OutputLine(channel: .localhost, text: note, timestamp: Date()), toRun: runID)
            }
            await logs.endRun(runID)
        }
        if let port = run.port {
            Task { [runID = run.id] in await self.watchPort(port, serviceID: serviceID, runID: runID) }
        } else {
            Task { [gracePeriod, runID = run.id] in
                try? await Task.sleep(for: gracePeriod)
                self.gracePeriodElapsed(serviceID, runID: runID)
            }
        }
        Task { [runID = run.id] in
            let termination = await tree.waitForShellExit()
            self.shellExited(serviceID, runID: runID, termination: termination)
            let exit = await tree.waitForExit()
            self.runEnded(serviceID, runID: runID, exit: exit)
        }
        return status(of: serviceID)
    }

    /// Records a Run that could not be launched at all, leaving the Service Failed.
    private func recordFailedLaunch(_ serviceID: UUID, origin: Origin, failure: LocalhostError) {
        var run = Run(id: UUID(), serviceID: serviceID, origin: origin, startedAt: Date())
        run.failure = failure
        run.endedAt = Date()
        broadcast(.runStarted(run))
        update(ServiceStatus(serviceID: serviceID, state: .failed, run: run))
        broadcast(.runEnded(run))
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

    /// Like `performStop`, but also ends a Run whose shell already exited and
    /// whose leftovers are still being cleaned up.
    private func performStopForQuit(_ serviceID: UUID, origin: Origin) async -> ServiceStatus {
        let state = status(of: serviceID).state
        if state == .starting || state == .running {
            return await performStop(serviceID, origin: origin)
        }
        guard let tree = trees[serviceID], let runID = statuses[serviceID]?.run?.id else { return status(of: serviceID) }
        let exit = await tree.stop()
        runEnded(serviceID, runID: runID, exit: exit)
        return status(of: serviceID)
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

    /// Keeps a Run Starting until its Port accepts connections. After the
    /// timeout it is Running with `isPortUnresponsive`, and the Port is still
    /// watched so the warning clears once it opens.
    private func watchPort(_ port: Int, serviceID: UUID, runID: UUID) async {
        let deadline = ContinuousClock.now + portTimeout
        while let status = statuses[serviceID], status.run?.id == runID,
              status.state == .starting || status.state == .running {
            if await PortProbe.isAccepting(port) {
                portAccepted(serviceID, runID: runID)
                return
            }
            if ContinuousClock.now >= deadline {
                portTimedOut(serviceID, runID: runID)
            }
            try? await Task.sleep(for: portProbeInterval)
        }
    }

    private func portAccepted(_ serviceID: UUID, runID: UUID) {
        var status = status(of: serviceID)
        guard status.run?.id == runID, status.state == .starting || status.state == .running else { return }
        status.state = .running
        status.run?.isPortUnresponsive = false
        update(status)
    }

    private func portTimedOut(_ serviceID: UUID, runID: UUID) {
        var status = status(of: serviceID)
        guard status.run?.id == runID, status.state == .starting else { return }
        status.state = .running
        status.run?.isPortUnresponsive = true
        update(status)
    }

    private func urlDetected(_ url: URL, serviceID: UUID, runID: UUID) {
        var status = status(of: serviceID)
        guard status.run?.id == runID, status.run?.endedAt == nil else { return }
        status.run?.detectedURL = url
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
        // A pending start stays Starting while the previous Run's leftovers end.
        if pendingStarts[serviceID] == nil {
            status.state = Self.stateAfterEnding(status.state, termination: exit.termination)
        }
        trees[serviceID] = nil
        runPorts[serviceID] = nil
        runRegistry.remove(runID)
        update(status)
        broadcast(.runEnded(run))
    }

    /// What localhost writes into a Run's log when it starts on another port.
    private static func logNote(forPort port: Int, change: PortChange) -> String {
        switch change.reason {
        case .portInUse(let holder?):
            "Port \(change.configuredPort) is in use by \(holder.name) (PID \(holder.pid)): starting on port \(port) instead"
        case .portInUse(nil):
            "Port \(change.configuredPort) is in use: starting on port \(port) instead"
        case .requested:
            "Starting on port \(port) instead of \(change.configuredPort), as requested"
        }
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
        // A one-shot command (e.g. a migration) may finish within Starting.
        case .starting, .running: termination == .exited(code: 0) ? .exited : .failed
        case .stopped, .exited, .failed: state
        }
    }

    // MARK: Plumbing

    /// Stores `status`, broadcasting `serviceStateChanged` if its state, Run,
    /// awaited Dependencies, Port warning or detected URL changed.
    private func update(_ status: ServiceStatus) {
        let previous = statuses[status.serviceID]
        statuses[status.serviceID] = status
        if status.state != previous?.state || status.run?.id != previous?.run?.id
            || status.awaitedDependencies != previous?.awaitedDependencies
            || status.run?.isPortUnresponsive != previous?.run?.isPortUnresponsive
            || status.run?.detectedURL != previous?.run?.detectedURL {
            broadcast(.serviceStateChanged(status))
        }
        for waiter in updateWaiters.removeValue(forKey: status.serviceID) ?? [] {
            waiter.resume()
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
