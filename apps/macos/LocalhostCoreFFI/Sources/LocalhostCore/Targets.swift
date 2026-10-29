import Foundation

// What the app runs Services through: this machine's shared core, the one
// `localhost mcp` drives too, so a Service an agent starts is Running here
// and the reverse. The app's executable is started as that core
// (`--daemon <storage root>`, see `runSharedCore`) when it isn't running.
// The methods mirror `ServiceManager`'s and `LogStore`'s, with Swift labels,
// Int ports and typed errors.

extension Targets {
    public convenience init(
        library: ProjectLibrary,
        storageRoot: URL = ProjectLibrary.defaultStorageRoot,
        isAutomaticPortMoveEnabled: Bool = true
    ) {
        let targets = Targets.ffiOpenShared(
            library: library,
            storageRoot: storageRoot.path(percentEncoded: false),
            automaticPortMove: isAutomaticPortMoveEnabled
        )
        self.init(unsafeFromHandle: targets.uniffiCloneHandle())
    }

    /// Connects to the shared core now, starting it if needed: the Runs it
    /// already has show at once, and its Orphans arrive in `foundOrphans()`.
    public func connect() async throws(LocalhostError) {
        try await core { try await ffiConnect() }
    }

    public func status(of serviceID: UUID) -> ServiceStatus {
        ffiStatus(serviceId: serviceID)
    }

    /// Every Service's status (those that never ran are left out), and a
    /// stream of every event after that.
    public func subscribe() -> (snapshot: [UUID: ServiceStatus], events: AsyncStream<ServiceEvent>) {
        let subscription = ffiSubscribe()
        let snapshot = Dictionary(subscription.snapshot.map { ($0.serviceId, $0) }, uniquingKeysWith: { $1 })
        return (snapshot, subscription.events.stream)
    }

    /// Starts the Service and what it depends on; `port` overrides its Port
    /// for this Run.
    @discardableResult
    public func start(_ serviceID: UUID, onPort port: Int? = nil, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        let port = try port.map { port throws(LocalhostError) in
            guard let port = UInt16(exactly: port), port > 0 else { throw .InvalidPort(Int64(port)) }
            return port
        }
        return try await core { try await ffiStart(serviceId: serviceID, onPort: port, origin: origin) }
    }

    @discardableResult
    public func stop(_ serviceID: UUID, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        try await core { try await ffiStop(serviceId: serviceID, origin: origin) }
    }

    @discardableResult
    public func restart(_ serviceID: UUID, origin: Origin) async throws(LocalhostError) -> ServiceStatus {
        try await core { try await ffiRestart(serviceId: serviceID, origin: origin) }
    }

    /// Starts every Service of the Project not excluded from Start All, in
    /// dependency order.
    public func startAll(inProject projectID: UUID, origin: Origin) async throws(LocalhostError) {
        try await core { try await ffiStartAll(projectId: projectID, origin: origin) }
    }

    public func stopAll(inProject projectID: UUID, origin: Origin) async throws(LocalhostError) {
        try await core { try await ffiStopAll(projectId: projectID, origin: origin) }
    }

    /// Stops every Run, whoever started it, e.g. when the app quits.
    public func stopAll(origin: Origin) async {
        await ffiStopEveryRun(origin: origin)
    }

    /// Asks the shared core to exit as soon as no client is connected, so
    /// the version an update relaunches starts a core of its own.
    public func retireSharedCore() async {
        await ffiRetireSharedCore()
    }

    public func setAutomaticPortMove(_ isEnabled: Bool) {
        ffiSetAutomaticPortMove(isEnabled: isEnabled)
    }

    /// Every process listening on a TCP port, with what owns it.
    public func listeners() async -> [Listener] {
        await ffiListeners()
    }

    public func stopListener(_ listener: Listener, origin: Origin) async throws(LocalhostError) {
        try await core { try await ffiStopListener(listener: listener, origin: origin) }
    }

    /// The first free port after the Service's own, `nil` if it has none.
    public func nextFreePort(for serviceID: UUID) async throws(LocalhostError) -> Int? {
        try await core { try await ffiNextFreePort(serviceId: serviceID) }.map(Int.init)
    }

    /// What the Service's active Run uses now, `nil` without one. Pass the
    /// previous sample of the same Run to get its CPU use since then.
    public func runStats(for serviceID: UUID, previous: RunStats? = nil) async throws(LocalhostError) -> RunStats? {
        try await core { try await ffiRunStats(serviceId: serviceID, previous: previous) }
    }

    // MARK: Logs

    /// The Run's buffered lines, and a stream of every line after them.
    public func subscribe(
        toRun runID: UUID
    ) async throws(LocalhostError) -> (lines: [OutputLine], updates: AsyncStream<OutputLine>) {
        let subscription = try await core { try await ffiSubscribeToRun(runId: runID) }
        return (subscription.lines, subscription.updates.stream)
    }

    /// The Services' latest Runs merged by time, and a stream of every line
    /// any of their Runs prints after that, including Runs that begin later.
    public func subscribe(
        toServices serviceIDs: Set<UUID>
    ) async throws(LocalhostError) -> (lines: [ServiceOutputLine], updates: AsyncStream<ServiceOutputLine>) {
        let subscription = try await core { try await ffiSubscribeToServices(serviceIds: Array(serviceIDs)) }
        return (subscription.lines, subscription.updates.stream)
    }

    /// Empties the latest Run's buffer, so viewers start from new output.
    public func clearBuffer(ofService serviceID: UUID) async throws(LocalhostError) {
        try await core { try await ffiClearBuffer(serviceId: serviceID) }
    }

    public func removeLogs(ofService serviceID: UUID) async throws(LocalhostError) {
        try await core { try await ffiRemoveLogs(serviceId: serviceID) }
    }

    // MARK: Orphans

    /// The Orphans the shared core reports as the app connects: Runs of a
    /// session that ended unexpectedly.
    public func foundOrphans() -> AsyncStream<[Orphan]> {
        ffiFoundOrphans().stream
    }

    /// Stops each Orphan's process group (SIGINT → SIGTERM → SIGKILL).
    public func stopOrphans(_ orphans: [Orphan]) async throws(LocalhostError) {
        try await core { try await ffiStopOrphans(orphans: orphans) }
    }

    /// Forgets the Orphans, leaving their processes running.
    public func leaveOrphans(_ orphans: [Orphan]) async throws(LocalhostError) {
        try await core { try await ffiLeaveOrphans(orphans: orphans) }
    }
}

/// Serves the shared core of `storageRoot` until no client has needed it
/// for a while: what the app's executable does when started with
/// `--daemon <storage root>`. Returns why it couldn't, or `nil`.
public func runSharedCore(storageRoot: URL) -> String? {
    LocalhostCore.runSharedCore(storageRoot: storageRoot.path(percentEncoded: false))
}
