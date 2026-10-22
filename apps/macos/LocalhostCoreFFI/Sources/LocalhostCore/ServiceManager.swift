import Foundation

// Starts and stops Services (Runs), tracks their States, moves Ports and
// lists Listeners. The core does the work; these add Swift labels, Int ports
// and typed errors.

extension ServiceManager {
    public convenience init(
        library: ProjectLibrary,
        logs: LogStore,
        runRegistry: RunRegistry,
        isAutomaticPortMoveEnabled: Bool = true
    ) {
        let manager = ServiceManager.ffiNew(
            library: library,
            logs: logs,
            runRegistry: runRegistry,
            automaticPortMove: isAutomaticPortMoveEnabled
        )
        self.init(unsafeFromHandle: manager.uniffiCloneHandle())
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

    /// Stops every Run, e.g. when the app quits.
    public func stopAll(origin: Origin) async {
        await ffiStopEveryRun(origin: origin)
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
}
