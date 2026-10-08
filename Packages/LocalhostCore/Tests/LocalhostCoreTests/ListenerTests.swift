import Darwin
import Foundation
import Testing
@testable import LocalhostCore

/// Listener discovery, stopping, port conflicts and automatic port moves,
/// against real processes. Listeners not started by localhost are `nc`
/// process trees launched directly with `ProcessTree`.
@Suite(.timeLimit(.minutes(1)))
final class ListenerTests: Sendable {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let quickStop = StopPolicy(interruptTimeout: .milliseconds(500), terminateTimeout: .milliseconds(500))
    let library: ProjectLibrary
    let manager: ServiceManager
    let project: Project
    let projectFolder: URL
    let outsideFolder: URL

    init() async throws {
        library = try ProjectLibrary(storageRoot: root)
        manager = ServiceManager(
            library: library,
            logs: LogStore(root: root),
            runRegistry: RunRegistry(root: root),
            baseEnvironment: ["PATH": "/usr/bin:/bin", "HOME": NSHomeDirectory(), "SHELL": "/bin/sh"],
            gracePeriod: .milliseconds(300),
            portTimeout: .seconds(5),
            portProbeInterval: .milliseconds(50),
            stopPolicy: quickStop
        )
        projectFolder = root.appending(path: "project")
        outsideFolder = root.appending(path: "elsewhere")
        for folder in [projectFolder, outsideFolder] {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        project = try await library.addProject(name: "App", key: Key("app"), path: projectFolder.path, origin: .mainWindow)
    }

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    /// Starts a process tree listening on `port` that localhost doesn't run,
    /// and waits until the port accepts connections.
    func listen(on port: Int, in directory: URL? = nil, _ commandLine: String? = nil) async throws -> ProcessTree {
        let tree = try ProcessTree.launch(
            LaunchConfiguration(
                commandLine: commandLine ?? "exec nc -lk \(port)",
                workingDirectory: directory ?? outsideFolder,
                environment: ["PATH": "/usr/bin:/bin"],
                shell: "/bin/sh"
            ),
            stopPolicy: quickStop
        )
        _ = try await listener(on: port)
        return tree
    }

    /// The Listener on `port`, once there is one. Not probed with a
    /// connection: `nc` stops listening while it serves one.
    func listener(on port: Int) async throws -> Listener {
        while true {
            if let listener = await manager.listeners().first(where: { $0.ports.contains(port) }) { return listener }
            try await Task.sleep(for: .milliseconds(20))
        }
    }

    func service(
        _ key: String, port: Int?, commandLine: String = "exec nc -lk $PORT",
        dependencies: [UUID] = [], portCanChange: Bool = true
    ) async throws -> Service {
        try await library.addService(
            toProject: project.id, name: key, key: Key(key), commandLine: commandLine,
            dependencies: dependencies, port: port, portCanChange: portCanChange, origin: .mainWindow
        )
    }

    // MARK: Discovery

    @Test func `discovers a Listener's command, ports and working directory, Unmanaged outside every Project`() async throws {
        let port = freePort()
        let tree = try await listen(on: port)

        let listener = try await listener(on: port)
        #expect(listener.pid == tree.pid)
        #expect(listener.name == "nc")
        #expect(listener.command == "nc -lk \(port)")
        #expect(listener.ports == [port])
        #expect(listener.workingDirectory == ListenerScanner.resolvedPath(outsideFolder.path))
        #expect(listener.owner == .unmanaged)
        #expect(listener.launchdLabel == nil)
        await tree.stop()
    }

    @Test func `a Listener working inside a Project's folder belongs to that Project`() async throws {
        let port = freePort()
        let subfolder = projectFolder.appending(path: "web")
        try FileManager.default.createDirectory(at: subfolder, withIntermediateDirectories: true)
        let tree = try await listen(on: port, in: subfolder)

        #expect(try await listener(on: port).owner == .project(project.id))
        await tree.stop()
    }

    @Test func `a Listener of a Run belongs to that Run`() async throws {
        let port = freePort()
        let service = try await service("web", port: port)
        let (_, events) = await manager.subscribe()
        let status = try await manager.start(service.id, origin: .mainWindow)
        for await case .serviceStateChanged(let status) in events where status.serviceID == service.id && status.state == .running { break }

        let runID = try #require(status.run?.id)
        #expect(try await listener(on: port).owner == .run(serviceID: service.id, runID: runID))
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `reads the running launchd jobs from launchctl list`() {
        let output = "PID\tStatus\tLabel\n412\t0\thomebrew.mxcl.postgresql@16\n-\t0\tcom.apple.idle\n77\t-9\tapplication.com.example.1.2\n"
        #expect(LaunchdJobs.parse(output) == [412: "homebrew.mxcl.postgresql@16", 77: "application.com.example.1.2"])
    }

    // MARK: Stopping

    @Test func `stops an Unmanaged Listener's process group`() async throws {
        let port = freePort()
        // With job control, nc gets a process group of its own, led by nc.
        let tree = try await listen(on: port, "set -m; nc -lk \(port) & wait")
        let listener = try await listener(on: port)
        #expect(listener.processGroupID == listener.pid)
        #expect(listener.pid != tree.pid)

        try await manager.stopListener(listener, origin: .mainWindow)

        #expect(await !PortProbe.isAccepting(port))
        #expect(kill(listener.pid, 0) == -1)
        _ = await tree.waitForExit()
    }

    @Test func `stops a session leader Listener and its descendants, not its whole session`() async throws {
        let port = freePort()
        // nc leads its session and group; the sleep it inherits is its child.
        let tree = try await listen(on: port, "sleep 1000 & exec nc -lk \(port)")
        let listener = try await listener(on: port)
        #expect(listener.pid == tree.pid)

        try await manager.stopListener(listener, origin: .mainWindow)

        // The tree has ended only once the sleep is gone too.
        _ = await tree.waitForExit()
        #expect(await !PortProbe.isAccepting(port))
    }

    @Test func `stopping a Run's Listener stops its Service`() async throws {
        let port = freePort()
        let service = try await service("web", port: port)
        try await manager.start(service.id, origin: .mainWindow)

        try await manager.stopListener(try await listener(on: port), origin: .mainWindow)

        #expect(await manager.status(of: service.id).state == .stopped)
        #expect(await manager.status(of: service.id).run?.stopOrigin == .mainWindow)
    }

    @Test func `refuses to stop localhost itself and launchd jobs`() async throws {
        let startTime = try #require(ProcessStartTime.of(getpid()))
        func listener(pid: pid_t, launchdLabel: String?) -> Listener {
            Listener(pid: pid, processGroupID: getpgrp(), name: "x", command: "x", ports: [1], workingDirectory: nil,
                     owner: .unmanaged, launchdLabel: launchdLabel, startTime: startTime)
        }
        await #expect(throws: LocalhostError.listenerIsLocalhost) {
            try await self.manager.stopListener(listener(pid: getpid(), launchdLabel: nil), origin: .mainWindow)
        }
        await #expect(throws: LocalhostError.listenerIsLaunchdJob(label: "homebrew.mxcl.redis")) {
            try await self.manager.stopListener(listener(pid: 1, launchdLabel: "homebrew.mxcl.redis"), origin: .mainWindow)
        }
    }

    // MARK: Port conflicts

    @Test func `a taken Port moves the Run to the next free port, saying why`() async throws {
        let port = freePort()
        let holder = try await listen(on: port)
        let service = try await service("web", port: port)
        let (_, events) = await manager.subscribe()

        let status = try await manager.start(service.id, origin: .mainWindow)
        let run = try #require(status.run)
        let movedPort = try #require(run.port)
        #expect(movedPort > port && movedPort <= port + 100)
        guard case .portInUse(let reportedHolder?) = run.portChange?.reason else {
            Issue.record("expected a move away from the holder, got \(String(describing: run.portChange))")
            return
        }
        #expect(reportedHolder.pid == holder.pid)
        #expect(run.portChange?.configuredPort == port)
        // The command got the moved port as $PORT.
        for await case .serviceStateChanged(let status) in events where status.serviceID == service.id && status.state == .running {
            #expect(status.run?.isPortUnresponsive == false)
            break
        }
        // The configuration keeps the Port.
        #expect(try await library.service(id: service.id).service.port == port)

        try await manager.stop(service.id, origin: .mainWindow)
        await holder.stop()
    }

    @Test func `with automatic moves off, a taken Port fails the start naming its owner`() async throws {
        let port = freePort()
        let holder = try await listen(on: port)
        let service = try await service("web", port: port)
        await manager.setAutomaticPortMove(false)

        let status = try await manager.start(service.id, origin: .mainWindow)

        #expect(status.state == .failed)
        guard case .portConflict(port, let owner?) = status.run?.failure else {
            Issue.record("expected portConflict, got \(String(describing: status.run?.failure))")
            return
        }
        #expect(owner.pid == holder.pid)
        #expect(status.run?.failure?.message == "Port \(port) is in use by nc (PID \(holder.pid)).")
        await holder.stop()
    }

    @Test func `a Service whose Port can't change is not moved`() async throws {
        let port = freePort()
        let holder = try await listen(on: port)
        let service = try await service("db", port: port, portCanChange: false)

        let status = try await manager.start(service.id, origin: .mainWindow)

        #expect(status.state == .failed)
        #expect(status.run?.failure == .portConflict(port: port, owner: try await listener(on: port)))
        await holder.stop()
    }

    @Test func `a Service with dependents is not moved`() async throws {
        let port = freePort()
        let holder = try await listen(on: port)
        let api = try await service("api", port: port)
        _ = try await service("web", port: nil, commandLine: "sleep 1000", dependencies: [api.id])

        let status = try await manager.start(api.id, origin: .mainWindow)

        #expect(status.state == .failed)
        #expect(status.run?.failure == .portConflict(port: port, owner: try await listener(on: port)))
        await holder.stop()
    }

    @Test func `a Port held inside the same Project is not moved away from`() async throws {
        let port = freePort()
        let holder = try await listen(on: port, in: projectFolder)
        let service = try await service("web", port: port)

        let status = try await manager.start(service.id, origin: .mainWindow)

        #expect(status.state == .failed)
        guard case .portConflict(port, let owner?) = status.run?.failure else {
            Issue.record("expected portConflict, got \(String(describing: status.run?.failure))")
            return
        }
        #expect(owner.owner == .project(project.id))
        await holder.stop()
    }

    @Test func `a move skips ports configured for other Services`() async throws {
        let port = freePort()
        let holder = try await listen(on: port)
        let service = try await service("web", port: port)
        _ = try await self.service("api", port: port + 1, commandLine: "sleep 1000")

        #expect(try await manager.nextFreePort(for: service.id).map { $0 > port + 1 } == true)
        let status = try await manager.start(service.id, origin: .mainWindow)
        let movedPort = try #require(status.run?.port)
        #expect(movedPort > port + 1)

        try await manager.stop(service.id, origin: .mainWindow)
        await holder.stop()
    }

    @Test func `starts on a requested port for that Run only`() async throws {
        let port = freePort()
        let otherPort = freePort()
        let service = try await service("web", port: port)

        let status = try await manager.start(service.id, onPort: otherPort, origin: .mainWindow)

        #expect(status.run?.port == otherPort)
        #expect(status.run?.portChange == PortChange(configuredPort: port, reason: .requested))
        try await manager.stop(service.id, origin: .mainWindow)
        let restarted = try await manager.start(service.id, origin: .mainWindow)
        #expect(restarted.run?.port == port)
        #expect(restarted.run?.portChange == nil)
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `in Start All a blocked Service fails with portConflict and its dependents with dependencyFailed`() async throws {
        let port = freePort()
        let holder = try await listen(on: port)
        let api = try await service("api", port: port)
        let web = try await service("web", port: nil, commandLine: "sleep 1000", dependencies: [api.id])

        try await manager.startAll(inProject: project.id, origin: .mainWindow)
        // Start All returns once every start has taken effect.
        while await manager.status(of: web.id).state == .starting { try await Task.sleep(for: .milliseconds(20)) }

        let apiStatus = await manager.status(of: api.id)
        #expect(apiStatus.state == .failed)
        guard case .portConflict(port, _) = apiStatus.run?.failure else {
            Issue.record("expected portConflict, got \(String(describing: apiStatus.run?.failure))")
            return
        }
        #expect(await manager.status(of: web.id).run?.failure == .dependencyFailed(api.key))
        await holder.stop()
    }
}
