import Darwin
import Foundation
import Testing
@testable import LocalhostCore

/// Service lifecycle against real processes. Each Service's command line is a fixture.
@Suite(.timeLimit(.minutes(1)))
final class ServiceManagerTests: Sendable {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let library: ProjectLibrary
    let manager: ServiceManager
    let project: Project

    init() async throws {
        library = try ProjectLibrary(storageRoot: root)
        manager = ServiceManager(
            library: library,
            logs: LogStore(root: root),
            runRegistry: RunRegistry(root: root),
            baseEnvironment: ["PATH": "/usr/bin:/bin", "HOME": NSHomeDirectory(), "SHELL": "/bin/sh"],
            gracePeriod: .milliseconds(300),
            stopPolicy: StopPolicy(interruptTimeout: .milliseconds(500), terminateTimeout: .milliseconds(500))
        )
        let projectFolder = root.appending(path: "project")
        try FileManager.default.createDirectory(at: projectFolder, withIntermediateDirectories: true)
        project = try await library.addProject(name: "App", key: Key("app"), path: projectFolder.path, origin: .mainWindow)
    }

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    func service(_ commandLine: String, workingDirectory: String = ".") async throws -> Service {
        let key = try Key("svc-\(UUID().uuidString.prefix(8).lowercased())")
        return try await library.addService(
            toProject: project.id, name: "Service", key: key, workingDirectory: workingDirectory,
            commandLine: commandLine, origin: .mainWindow
        )
    }

    /// Collects the events for one Service until `isLast` returns true.
    func events(
        for serviceID: UUID,
        _ events: AsyncStream<ServiceEvent>,
        until isLast: (ServiceEvent) -> Bool
    ) async -> [ServiceEvent] {
        var collected: [ServiceEvent] = []
        for await event in events where event.serviceID == serviceID {
            collected.append(event)
            if isLast(event) { break }
        }
        return collected
    }

    func states(_ events: [ServiceEvent]) -> [ServiceState] {
        events.compactMap {
            if case .serviceStateChanged(let status) = $0 { status.state } else { nil }
        }
    }

    func waitForState(_ state: ServiceState, of serviceID: UUID, in events: AsyncStream<ServiceEvent>) async {
        _ = await self.events(for: serviceID, events) {
            if case .serviceStateChanged(let status) = $0 { status.state == state } else { false }
        }
    }

    // MARK: Starting

    @Test func `a service that has never run is Stopped`() async throws {
        let service = try await service("sleep 1000")
        let status = await manager.status(of: service.id)
        #expect(status.state == .stopped)
        #expect(status.run == nil)
    }

    @Test func `start goes Starting, then Running after the grace period`() async throws {
        let service = try await service("sleep 1000")
        let (_, events) = await manager.subscribe()

        let started = ContinuousClock.now
        let status = try await manager.start(service.id, origin: .mainWindow)
        #expect(status.state == .starting)

        let seen = await self.events(for: service.id, events) {
            if case .serviceStateChanged(let status) = $0 { status.state == .running } else { false }
        }
        #expect(ContinuousClock.now - started >= .milliseconds(300))

        guard case .runStarted(let run) = seen.first else {
            Issue.record("expected runStarted first, got \(seen)")
            return
        }
        #expect(run.serviceID == service.id)
        #expect(run.origin == .mainWindow)
        #expect(run.endedAt == nil)
        #expect(states(seen) == [.starting, .running])

        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `start is idempotent while Starting or Running`() async throws {
        let service = try await service("sleep 1000")
        let first = try await manager.start(service.id, origin: .mainWindow)
        let second = try await manager.start(service.id, origin: .menuBar)
        #expect(first.run?.id == second.run?.id)

        try await Task.sleep(for: .milliseconds(400))
        let third = try await manager.start(service.id, origin: .menuBar)
        #expect(third.state == .running)
        #expect(third.run?.id == first.run?.id)

        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `concurrent starts launch a single Run`() async throws {
        let service = try await service("sleep 1000")
        let (_, events) = await manager.subscribe()

        async let a = manager.start(service.id, origin: .mainWindow)
        async let b = manager.start(service.id, origin: .menuBar)
        let (first, second) = try await (a, b)
        #expect(first.run?.id == second.run?.id)

        try await manager.stop(service.id, origin: .mainWindow)
        let seen = await self.events(for: service.id, events) {
            if case .runEnded = $0 { true } else { false }
        }
        #expect(seen.filter { if case .runStarted = $0 { true } else { false } }.count == 1)
    }

    @Test func `uses the service's working directory and environment`() async throws {
        let folder = project.path + "/web"
        try FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true)
        let service = try await library.addService(
            toProject: project.id, name: "Web", key: Key("web"), workingDirectory: "web",
            commandLine: #"pwd -P > out.txt; echo "$PORT $FORCE_COLOR" >> out.txt"#,
            environment: ["PORT": "3000"], origin: .mainWindow
        )
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)
        _ = await self.events(for: service.id, events) { if case .runEnded = $0 { true } else { false } }

        let output = try String(contentsOfFile: folder + "/out.txt", encoding: .utf8)
        #expect(output.hasSuffix("/project/web\n3000 1\n"))
    }

    @Test func `starting an unknown service throws`() async throws {
        let id = UUID()
        await #expect(throws: LocalhostError.serviceNotFound(id)) {
            try await self.manager.start(id, origin: .mainWindow)
        }
    }

    // MARK: Ending on its own

    @Test func `dying during Starting is Failed, even with exit code 0`() async throws {
        let service = try await service("exit 0")
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)

        let seen = await self.events(for: service.id, events) { if case .runEnded = $0 { true } else { false } }
        #expect(states(seen) == [.starting, .failed])
        #expect(await manager.status(of: service.id).state == .failed)
    }

    @Test(arguments: [
        ("sleep 0.5; exit 0", ServiceState.exited, Termination.exited(code: 0)),
        ("sleep 0.5; exit 1", ServiceState.failed, Termination.exited(code: 1)),
        ("sleep 0.5; kill -KILL $$", ServiceState.failed, Termination.signaled(SIGKILL)),
    ])
    func `ending on its own after Running is Exited on success, Failed otherwise`(
        commandLine: String, expectedState: ServiceState, expectedTermination: Termination
    ) async throws {
        let service = try await service(commandLine)
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)

        let seen = await self.events(for: service.id, events) { if case .runEnded = $0 { true } else { false } }
        #expect(states(seen) == [.starting, .running, expectedState])
        guard case .runEnded(let run) = seen.last else { return }
        #expect(run.termination == expectedTermination)
        #expect(run.failure == nil)
        #expect(run.stopOrigin == nil)
    }

    @Test func `an unknown command is Failed with commandNotFound`() async throws {
        let service = try await service("definitely-not-a-command-\(UUID().uuidString.prefix(8))")
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)

        let seen = await self.events(for: service.id, events) { if case .runEnded = $0 { true } else { false } }
        guard case .runEnded(let run) = seen.last else { return }
        #expect(run.termination == .exited(code: 127))
        #expect(run.failure == .commandNotFound)
        #expect(await manager.status(of: service.id).state == .failed)
    }

    @Test func `a launch failure is Failed with a Run that records the error`() async throws {
        let service = try await service("sleep 1000", workingDirectory: "missing")
        let status = try await manager.start(service.id, origin: .menuBar)

        #expect(status.state == .failed)
        let run = try #require(status.run)
        #expect(run.failure == .workingDirectoryMissing(project.path + "/missing"))
        #expect(run.termination == nil)
        #expect(run.origin == .menuBar)
        #expect(run.endedAt != nil)
    }

    @Test func `never Running once the shell has exited, even while leftovers are being cleaned up`() async throws {
        let service = try await service(leftoverIgnoringSignals(thenParent: "sleep 0.5; exit 1"))
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)
        await waitForState(.running, of: service.id, in: events)

        await waitForState(.failed, of: service.id, in: events)
        let status = await manager.status(of: service.id)
        #expect(status.state == .failed)
        // The Run is still ending: its leftover ignores SIGTERM and needs SIGKILL.
        #expect(status.run?.endedAt == nil)

        let seen = await self.events(for: service.id, events) { if case .runEnded = $0 { true } else { false } }
        guard case .runEnded(let run) = seen.last else { return }
        #expect(run.wasForceKilled)
        #expect(await manager.status(of: service.id).state == .failed)
    }

    // MARK: Stopping

    @Test func `a requested stop ends Stopped, whatever the exit code`() async throws {
        let service = try await service("trap 'exit 3' INT; while :; do sleep 0.05; done")
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)
        await waitForState(.running, of: service.id, in: events)

        let status = try await manager.stop(service.id, origin: .menuBar)

        #expect(status.state == .stopped)
        let run = try #require(status.run)
        #expect(run.termination == .exited(code: 3))
        #expect(run.origin == .mainWindow)
        #expect(run.stopOrigin == .menuBar)
        #expect(run.endedAt != nil)
        #expect(run.startedAt <= run.endedAt!)
        let seen = await self.events(for: service.id, events) { if case .runEnded = $0 { true } else { false } }
        #expect(states(seen) == [.stopping, .stopped])
    }

    @Test func `stopping during Starting ends Stopped`() async throws {
        let service = try await service("sleep 1000")
        try await manager.start(service.id, origin: .mainWindow)
        let status = try await manager.stop(service.id, origin: .mainWindow)
        #expect(status.state == .stopped)
    }

    @Test func `stop is a no-op when nothing is running`() async throws {
        let service = try await service("sleep 1000")
        #expect(try await manager.stop(service.id, origin: .mainWindow).state == .stopped)

        let exited = try await self.service("exit 1")
        let (_, events) = await manager.subscribe()
        try await manager.start(exited.id, origin: .mainWindow)
        _ = await self.events(for: exited.id, events) { if case .runEnded = $0 { true } else { false } }
        #expect(try await manager.stop(exited.id, origin: .mainWindow).state == .failed)
    }

    @Test func `concurrent stops both return Stopped`() async throws {
        let service = try await service("sleep 1000")
        try await manager.start(service.id, origin: .mainWindow)

        async let a = manager.stop(service.id, origin: .mainWindow)
        async let b = manager.stop(service.id, origin: .menuBar)
        let (first, second) = try await (a, b)
        #expect(first.state == .stopped)
        #expect(second.state == .stopped)
        #expect(first.run?.id == second.run?.id)
    }

    @Test func `start while Stopping waits for the stop, then starts a new Run`() async throws {
        let service = try await service("trap '' INT; while :; do sleep 0.05; done")
        let (_, events) = await manager.subscribe()
        let first = try await manager.start(service.id, origin: .mainWindow)
        await waitForState(.running, of: service.id, in: events)

        async let stopped = manager.stop(service.id, origin: .mainWindow)
        try await Task.sleep(for: .milliseconds(100))
        #expect(await manager.status(of: service.id).state == .stopping)
        let restarted = try await manager.start(service.id, origin: .mainWindow)

        #expect(try await stopped.state == .stopped)
        #expect(restarted.state == .starting)
        #expect(restarted.run?.id != first.run?.id)
        try await manager.stop(service.id, origin: .mainWindow)
    }

    // MARK: Restarting

    @Test func `restart stops the current Run and starts a new one`() async throws {
        let service = try await service("sleep 1000")
        let first = try await manager.start(service.id, origin: .mainWindow)
        let (_, events) = await manager.subscribe()

        let restarted = try await manager.restart(service.id, origin: .menuBar)

        #expect(restarted.state == .starting)
        #expect(restarted.run?.id != first.run?.id)
        #expect(restarted.run?.origin == .menuBar)
        let seen = await self.events(for: service.id, events) {
            if case .serviceStateChanged(let status) = $0 { status.state == .running } else { false }
        }
        #expect(states(seen) == [.stopping, .stopped, .starting, .running])
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `restart starts a service that is not running`() async throws {
        let service = try await service("sleep 1000")
        #expect(try await manager.restart(service.id, origin: .mainWindow).state == .starting)
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `each start reads the current service definition`() async throws {
        let service = try await service("sleep 1000")
        try await library.editService(id: service.id, commandLine: "exit 7", origin: .mainWindow)
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)

        let seen = await self.events(for: service.id, events) { if case .runEnded = $0 { true } else { false } }
        guard case .runEnded(let run) = seen.last else { return }
        #expect(run.termination == .exited(code: 7))
    }

    // MARK: Observing

    @Test func `subscribe returns the current snapshot with the stream`() async throws {
        let service = try await service("sleep 1000")
        try await manager.start(service.id, origin: .mainWindow)

        let (snapshot, _) = await manager.subscribe()
        #expect(snapshot[service.id]?.state == .starting)
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test @MainActor func `the main-actor store mirrors service states`() async throws {
        let service = try await service("sleep 1000")
        let store = ServiceStatusStore(manager: manager)
        #expect(store.status(of: service.id).state == .stopped)

        try await manager.start(service.id, origin: .mainWindow)
        try await waitUntil { store.status(of: service.id).state == .running }

        try await manager.stop(service.id, origin: .mainWindow)
        try await waitUntil { store.status(of: service.id).state == .stopped }
        #expect(store.status(of: service.id).run?.stopOrigin == .mainWindow)
    }
}

/// Polls `condition` on the main actor until it holds.
@MainActor
func waitUntil(timeout: Duration = .seconds(5), _ condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now + timeout
    while !condition() {
        guard ContinuousClock.now < deadline else {
            Issue.record("condition not met within \(timeout)")
            return
        }
        try await Task.sleep(for: .milliseconds(20))
    }
}
