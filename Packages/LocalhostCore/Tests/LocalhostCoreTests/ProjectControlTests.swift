import Foundation
import Testing
@testable import LocalhostCore

/// Dependencies, Start All and Stop All against real processes.
@Suite(.timeLimit(.minutes(1)))
final class ProjectControlTests: Sendable {
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

    func key(_ raw: String) -> Key {
        try! Key(raw)
    }

    func service(
        _ key: String, _ commandLine: String = "sleep 1000", dependsOn dependencies: [Service] = [],
        excludedFromStartAll: Bool = false
    ) async throws -> Service {
        try await library.addService(
            toProject: project.id, name: key, key: Key(key), commandLine: commandLine,
            dependencies: dependencies.map(\.id), isExcludedFromStartAll: excludedFromStartAll, origin: .mainWindow
        )
    }

    /// Collects events of every Service until `isLast` returns true.
    func events(_ events: AsyncStream<ServiceEvent>, until isLast: (ServiceEvent) -> Bool) async -> [ServiceEvent] {
        var collected: [ServiceEvent] = []
        for await event in events {
            collected.append(event)
            if isLast(event) { break }
        }
        return collected
    }

    func reached(_ state: ServiceState, _ service: Service) -> (ServiceEvent) -> Bool {
        { if case .serviceStateChanged(let status) = $0 { status.serviceID == service.id && status.state == state } else { false } }
    }

    /// The position of the first event matching `predicate`.
    func index(in events: [ServiceEvent], _ predicate: (ServiceEvent) -> Bool) -> Int? {
        events.firstIndex(where: predicate)
    }

    func runStarted(_ service: Service) -> (ServiceEvent) -> Bool {
        { if case .runStarted(let run) = $0 { run.serviceID == service.id } else { false } }
    }

    func stopEverything() async {
        await manager.stopAll(origin: .mainWindow)
    }

    // MARK: Starting with Dependencies

    @Test func `starting a Service starts its Dependency first and waits until it is Running`() async throws {
        let database = try await service("database")
        let api = try await service("api", dependsOn: [database])
        let (_, stream) = await manager.subscribe()

        let status = try await manager.start(api.id, origin: .mainWindow)
        #expect(status.state == .starting)
        #expect(status.run != nil)
        #expect(status.awaitedDependencies.isEmpty)

        let seen = await events(stream, until: runStarted(api))
        let waiting = seen.contains {
            if case .serviceStateChanged(let status) = $0 {
                status.serviceID == api.id && status.state == .starting && status.awaitedDependencies == [database.id]
            } else { false }
        }
        #expect(waiting)
        let databaseRunning = try #require(index(in: seen, reached(.running, database)))
        let apiLaunched = try #require(index(in: seen, runStarted(api)))
        #expect(databaseRunning < apiLaunched)
        await stopEverything()
    }

    @Test func `a Dependency that Exits satisfies its dependents`() async throws {
        let migrate = try await service("migrate", "exit 0")
        let api = try await service("api", dependsOn: [migrate])

        let status = try await manager.start(api.id, origin: .mainWindow)

        #expect(status.state == .starting)
        #expect(status.run != nil)
        #expect(await manager.status(of: migrate.id).state == .exited)
        await stopEverything()
    }

    @Test func `a Failed Dependency leaves its dependents Failed with dependencyFailed`() async throws {
        let database = try await service("database", "exit 1")
        let api = try await service("api", dependsOn: [database])
        let web = try await service("web", dependsOn: [api])

        let status = try await manager.start(web.id, origin: .menuBar)

        #expect(status.state == .failed)
        #expect(status.run?.failure == .dependencyFailed(key("api")))
        #expect(status.run?.termination == nil)
        #expect(status.run?.origin == .menuBar)
        #expect(await manager.status(of: api.id).run?.failure == .dependencyFailed(key("database")))
    }

    @Test func `a Dependency that is already Running is not restarted`() async throws {
        let database = try await service("database")
        let api = try await service("api", dependsOn: [database])
        let (_, stream) = await manager.subscribe()
        let first = try await manager.start(database.id, origin: .mainWindow)
        _ = await events(stream, until: reached(.running, database))

        try await manager.start(api.id, origin: .mainWindow)

        #expect(await manager.status(of: database.id).run?.id == first.run?.id)
        await stopEverything()
    }

    @Test func `stopping a Service waiting for its Dependency ends it Stopped and lets the Dependency start`() async throws {
        let database = try await service("database")
        let api = try await service("api", dependsOn: [database])
        let (_, stream) = await manager.subscribe()

        async let started = manager.start(api.id, origin: .mainWindow)
        _ = await events(stream, until: runStarted(database))
        let stopped = try await manager.stop(api.id, origin: .mainWindow)

        #expect(stopped.state == .stopped)
        #expect(stopped.run == nil)
        #expect(try await started.state == .stopped)
        _ = await events(stream, until: reached(.running, database))
        #expect(await manager.status(of: api.id).run == nil)
        await stopEverything()
    }

    @Test func `stopping a Dependency others wait on ends them Stopped`() async throws {
        let database = try await service("database")
        let api = try await service("api", dependsOn: [database])
        let (_, stream) = await manager.subscribe()

        async let started = manager.start(api.id, origin: .mainWindow)
        _ = await events(stream, until: runStarted(database))
        try await manager.stop(database.id, origin: .mainWindow)

        let status = try await started
        #expect(status.state == .stopped)
        #expect(status.run == nil)
    }

    @Test func `restarting a Dependency leaves its dependents running`() async throws {
        let database = try await service("database")
        let api = try await service("api", dependsOn: [database])
        let (_, stream) = await manager.subscribe()
        let apiRun = try await manager.start(api.id, origin: .mainWindow).run
        _ = await events(stream, until: reached(.running, api))

        try await manager.restart(database.id, origin: .mainWindow)

        let status = await manager.status(of: api.id)
        #expect(status.state == .running)
        #expect(status.run?.id == apiRun?.id)
        await stopEverything()
    }

    // MARK: Start All and Stop All

    @Test func `Start All starts every included Service after its Dependencies`() async throws {
        let database = try await service("database")
        let migrate = try await service("migrate", "exit 0", excludedFromStartAll: true)
        let api = try await service("api", dependsOn: [database, migrate])
        let web = try await service("web", dependsOn: [api])
        let seed = try await service("seed", "exit 0", excludedFromStartAll: true)
        let (_, stream) = await manager.subscribe()

        try await manager.startAll(inProject: project.id, origin: .menuBar)

        let seen = await events(stream, until: runStarted(web))
        let order = try [database, migrate, api, web].map { try #require(index(in: seen, runStarted($0))) }
        #expect(order[0] < order[2] && order[1] < order[2] && order[2] < order[3])
        #expect(await manager.status(of: seed.id).run == nil)
        #expect(await manager.status(of: web.id).run?.origin == .menuBar)
        await stopEverything()
    }

    @Test func `Start All runs an Exited Service again before its dependents`() async throws {
        let migrate = try await service("migrate", "exit 0")
        let api = try await service("api", dependsOn: [migrate])
        try await manager.startAll(inProject: project.id, origin: .mainWindow)
        try await manager.stopAll(inProject: project.id, origin: .mainWindow)
        #expect(await manager.status(of: migrate.id).state == .exited)
        let (_, stream) = await manager.subscribe()

        try await manager.startAll(inProject: project.id, origin: .mainWindow)

        let seen = await events(stream, until: runStarted(api))
        #expect(index(in: seen, runStarted(migrate)) != nil)
        await stopEverything()
    }

    @Test func `Stop All stops dependents before their Dependencies`() async throws {
        let database = try await service("database")
        let web = try await service("web", dependsOn: [database])
        let (_, stream) = await manager.subscribe()
        try await manager.startAll(inProject: project.id, origin: .mainWindow)
        _ = await events(stream, until: reached(.running, web))

        try await manager.stopAll(inProject: project.id, origin: .mainWindow)

        let seen = await events(stream, until: reached(.stopped, database))
        let webStopped = try #require(index(in: seen, reached(.stopped, web)))
        let databaseStopping = try #require(index(in: seen, reached(.stopping, database)))
        #expect(webStopped < databaseStopping)
    }

    @Test func `Stop All cancels starts still waiting for their Dependencies`() async throws {
        let database = try await service("database")
        let api = try await service("api", dependsOn: [database])
        let (_, stream) = await manager.subscribe()

        async let startedAll: Void = manager.startAll(inProject: project.id, origin: .mainWindow)
        _ = await events(stream, until: runStarted(database))
        try await manager.stopAll(inProject: project.id, origin: .mainWindow)
        try await startedAll

        #expect(await manager.status(of: api.id).state == .stopped)
        #expect(await manager.status(of: api.id).run == nil)
        #expect(await manager.status(of: database.id).state == .stopped)
    }
}
