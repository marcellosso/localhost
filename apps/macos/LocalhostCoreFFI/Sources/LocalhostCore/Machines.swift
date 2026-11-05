import Foundation
import Observation

// Other Machines: computers whose localhost this app shows, reached with the
// system's `ssh`. The core keeps each one connected, reconnecting after a
// loss, and mirrors its Projects and statuses; changes there are made as
// `Origin.machine`, which the other Machine decides. These add Swift labels,
// Int ports, typed errors and streams.

extension Machines {
    /// The Machines saved in `storageRoot`'s `machines.json`.
    public convenience init(storageRoot: URL = ProjectLibrary.defaultStorageRoot) throws(LocalhostError) {
        let machines = try core { try Machines.ffiOpen(storageRoot: storageRoot.path(percentEncoded: false)) }
        self.init(unsafeFromHandle: machines.uniffiCloneHandle())
    }

    public func machines() -> [Machine] {
        ffiMachines()
    }

    /// Every Machine as it is now, and a stream of every change after that.
    public func subscribe() -> (snapshot: [MachineSnapshot], events: AsyncStream<MachinesEvent>) {
        let subscription = ffiSubscribe()
        return (subscription.machines, subscription.events.stream)
    }

    /// Connects to the Machine `ssh` reaches as `destination` (a `Host` of
    /// `~/.ssh/config`, or `user@host`), and only once that works remembers
    /// it and keeps it connected. Throws `.MachineUnavailable` with why not.
    @discardableResult
    public func add(destination: String, name: String? = nil) async throws(LocalhostError) -> Machine {
        try await core { try await ffiAdd(name: name, destination: destination) }
    }

    /// An empty name is its destination.
    @discardableResult
    public func rename(_ machineID: UUID, to name: String) async throws(LocalhostError) -> Machine {
        try await core { try await ffiRename(machineId: machineID, name: name) }
    }

    /// Disconnects and forgets the Machine. Nothing changes on it.
    public func remove(_ machineID: UUID) async throws(LocalhostError) {
        try await core { try await ffiRemove(machineId: machineID) }
    }

    /// Connects, and keeps reconnecting after a loss; while it waits to try
    /// again, tries now. The Machine's state says how it goes.
    public func connect(_ machineID: UUID) throws(LocalhostError) {
        try core { try ffiConnect(machineId: machineID) }
    }

    /// Ends the connection, even a working one, and makes it again.
    public func reconnect(_ machineID: UUID) throws(LocalhostError) {
        try core { try ffiReconnect(machineId: machineID) }
    }

    public func disconnect(_ machineID: UUID) {
        ffiDisconnect(machineId: machineID)
    }

    /// The computer woke or its network changed: tries waiting connections now.
    public func wake() {
        ffiWake()
    }

    // MARK: Its Services

    @discardableResult
    public func start(_ serviceID: UUID, on machineID: UUID, onPort port: Int? = nil) async throws(LocalhostError) -> ServiceStatus {
        let port = try port.map { port throws(LocalhostError) in
            guard let port = UInt16(exactly: port), port > 0 else { throw .InvalidPort(Int64(port)) }
            return port
        }
        return try await core { try await ffiStart(machineId: machineID, serviceId: serviceID, onPort: port) }
    }

    @discardableResult
    public func stop(_ serviceID: UUID, on machineID: UUID) async throws(LocalhostError) -> ServiceStatus {
        try await core { try await ffiStop(machineId: machineID, serviceId: serviceID) }
    }

    @discardableResult
    public func restart(_ serviceID: UUID, on machineID: UUID) async throws(LocalhostError) -> ServiceStatus {
        try await core { try await ffiRestart(machineId: machineID, serviceId: serviceID) }
    }

    public func startAll(inProject projectID: UUID, on machineID: UUID) async throws(LocalhostError) {
        try await core { try await ffiStartAll(machineId: machineID, projectId: projectID) }
    }

    public func stopAll(inProject projectID: UUID, on machineID: UUID) async throws(LocalhostError) {
        try await core { try await ffiStopAll(machineId: machineID, projectId: projectID) }
    }

    /// The URL to open for the Service here, its Port forwarded to the same
    /// port on this Mac first; `nil` when it has nothing to open. Throws
    /// `.PortConflict` when that port is taken here.
    public func openURL(ofService serviceID: UUID, on machineID: UUID) async throws(LocalhostError) -> URL? {
        let url = try await core { try await ffiOpenService(machineId: machineID, serviceId: serviceID) }
        return url.flatMap { URL(string: $0) }
    }

    /// The Machine's logs, for a log view.
    public func logs(on machineID: UUID) -> MachineLogs {
        MachineLogs(machines: self, machineID: machineID)
    }

    /// The hosts named in `~/.ssh/config` (following `Include`, without
    /// patterns), to suggest as a destination. Only read.
    public static func sshConfigHosts() async -> [String] {
        await LocalhostCore.sshConfigHosts()
    }
}

extension Machine: Identifiable {}

extension MachineSnapshot: Identifiable {
    public var id: UUID { machine.id }
}

extension MachineProblem {
    /// Why `machine` can't be reached, and what to do next.
    public func message(machine: String) -> String {
        machineProblemMessage(problem: self, machine: machine)
    }
}

extension MachinesEvents {
    var stream: AsyncStream<MachinesEvent> { makeStream(next: next, close: close) }
}

// MARK: Logs

/// Where a log view reads Runs' output: this Mac's shared core (`Targets`),
/// or another Machine (`MachineLogs`).
public protocol LogSource: Sendable {
    func subscribe(toRun runID: UUID) async throws(LocalhostError) -> (lines: [OutputLine], updates: AsyncStream<OutputLine>)
    func subscribe(
        toServices serviceIDs: Set<UUID>
    ) async throws(LocalhostError) -> (lines: [ServiceOutputLine], updates: AsyncStream<ServiceOutputLine>)
}

extension Targets: LogSource {}

/// One Machine's logs, through its connection.
public struct MachineLogs: LogSource {
    public let machines: Machines
    public let machineID: UUID

    public func subscribe(toRun runID: UUID) async throws(LocalhostError) -> (lines: [OutputLine], updates: AsyncStream<OutputLine>) {
        let subscription = try await core { try await machines.ffiSubscribeToRun(machineId: machineID, runId: runID) }
        return (subscription.lines, subscription.updates.stream)
    }

    public func subscribe(
        toServices serviceIDs: Set<UUID>
    ) async throws(LocalhostError) -> (lines: [ServiceOutputLine], updates: AsyncStream<ServiceOutputLine>) {
        let subscription = try await core {
            try await machines.ffiSubscribeToServices(machineId: machineID, serviceIds: Array(serviceIDs))
        }
        return (subscription.lines, subscription.updates.stream)
    }
}

// MARK: Mirror

/// A main-actor mirror of every other Machine, for SwiftUI to observe: each
/// one's state, Projects and statuses. It only reflects `Machines`; changes
/// go through it.
@MainActor
@Observable
public final class MachinesStore {
    /// One Machine as the app shows it. Its Projects and statuses are those
    /// of its last connection, kept while it reconnects.
    public struct Shown: Identifiable, Sendable {
        public var machine: Machine
        public var state: MachineState
        public var projects: [Project]
        public var statuses: [UUID: ServiceStatus]

        public var id: UUID { machine.id }

        public var isConnected: Bool {
            if case .connected = state { true } else { false }
        }

        public func status(of serviceID: UUID) -> ServiceStatus {
            statuses[serviceID] ?? .neverRun(serviceID)
        }

        public func project(_ projectID: UUID) -> Project? {
            projects.first { $0.id == projectID }
        }

        public func service(_ serviceID: UUID) -> (project: Project, service: Service)? {
            for project in projects {
                if let service = project.service(id: serviceID) { return (project, service) }
            }
            return nil
        }
    }

    /// In the order they were added.
    public private(set) var machines: [Shown] = []
    @ObservationIgnored private var observation: Task<Void, Never>?

    public init(machines source: Machines) {
        observation = Task { [weak self] in
            let (snapshot, events) = source.subscribe()
            self?.machines = snapshot.map { Shown($0) }
            for await event in events {
                guard let self else { return }
                self.apply(event)
            }
        }
    }

    isolated deinit {
        observation?.cancel()
    }

    public func machine(_ machineID: UUID) -> Shown? {
        machines.first { $0.id == machineID }
    }

    private func apply(_ event: MachinesEvent) {
        switch event {
        case .machinesChanged(let list):
            machines = list.map { machine in
                if var shown = self.machine(machine.id) {
                    shown.machine = machine
                    return shown
                }
                return Shown(machine: machine, state: .notConnected, projects: [], statuses: [:])
            }
        case .stateChanged(let machineID, let state):
            update(machineID) { $0.state = state }
        case .projectsChanged(let machineID, let projects):
            update(machineID) { $0.projects = projects }
        case .statusesChanged(let machineID, let statuses):
            update(machineID) { $0.statuses = Dictionary(statuses.map { ($0.serviceID, $0) }, uniquingKeysWith: { $1 }) }
        case .serviceChanged(let machineID, let event):
            update(machineID) { shown in
                switch event {
                case .serviceStateChanged(let status):
                    shown.statuses[status.serviceID] = status
                case .runStarted(let run), .runEnded(let run):
                    if shown.statuses[run.serviceID]?.run?.id == run.id {
                        shown.statuses[run.serviceID]?.run = run
                    }
                }
            }
        }
    }

    private func update(_ machineID: UUID, _ change: (inout Shown) -> Void) {
        guard let index = machines.firstIndex(where: { $0.id == machineID }) else { return }
        change(&machines[index])
    }
}

extension MachinesStore.Shown {
    init(_ snapshot: MachineSnapshot) {
        self.init(
            machine: snapshot.machine,
            state: snapshot.state,
            projects: snapshot.projects,
            statuses: Dictionary(snapshot.statuses.map { ($0.serviceID, $0) }, uniquingKeysWith: { $1 })
        )
    }
}
