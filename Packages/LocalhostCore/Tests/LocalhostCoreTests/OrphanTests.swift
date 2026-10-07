import Darwin
import Foundation
import Testing
@testable import LocalhostCore

/// Orphan recovery against real processes. A "previous session" is a
/// ServiceManager whose RunRegistry is replaced by a fresh one on the same
/// storage root, as after a crash and relaunch.
@Suite(.timeLimit(.minutes(1)))
final class OrphanTests: Sendable {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let quickStop = StopPolicy(interruptTimeout: .milliseconds(300), terminateTimeout: .milliseconds(300))
    let library: ProjectLibrary
    let previousSession: ServiceManager
    let project: Project

    init() async throws {
        library = try ProjectLibrary(storageRoot: root)
        previousSession = ServiceManager(
            library: library,
            logs: LogStore(root: root),
            runRegistry: RunRegistry(root: root),
            baseEnvironment: ["PATH": "/usr/bin:/bin", "HOME": NSHomeDirectory(), "SHELL": "/bin/sh"],
            gracePeriod: .milliseconds(200),
            stopPolicy: quickStop
        )
        project = try await library.addProject(name: "App", key: Key("app"), path: root.path, origin: .mainWindow)
    }

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    var runsDirectory: URL { root.appending(path: "runs") }

    func recordFiles() -> [String] {
        (try? FileManager.default.contentsOfDirectory(atPath: runsDirectory.path)) ?? []
    }

    /// Starts a Service in the previous session and returns it with its Run.
    func startInPreviousSession(_ commandLine: String = "sleep 1000") async throws -> (Service, Run) {
        let service = try await library.addService(
            toProject: project.id, name: "Svc", key: Key("svc-\(UUID().uuidString.prefix(6).lowercased())"),
            commandLine: commandLine, origin: .mainWindow
        )
        let status = try await previousSession.start(service.id, origin: .mainWindow)
        return (service, try #require(status.run))
    }

    // MARK: Recording

    @Test func `records each active Run and clears the record when it ends`() async throws {
        let (service, run) = try await startInPreviousSession()
        #expect(recordFiles() == ["\(run.id.uuidString).json"])

        try await previousSession.stop(service.id, origin: .mainWindow)
        #expect(recordFiles().isEmpty)
    }

    @Test func `clears the record when a Run ends on its own`() async throws {
        let (_, events) = await previousSession.subscribe()
        let (service, _) = try await startInPreviousSession("exit 0")
        if await previousSession.status(of: service.id).run?.endedAt == nil {
            for await event in events where event.serviceID == service.id {
                if case .runEnded = event { break }
            }
        }
        #expect(recordFiles().isEmpty)
    }

    @Test func `records the pid and the process start time`() throws {
        let registry = RunRegistry(root: root)
        let pid = getpid()
        let startTime = try #require(ProcessStartTime.of(pid))
        let run = Run(id: UUID(), serviceID: UUID(), origin: .mainWindow, startedAt: Date())
        registry.record(run, pid: pid, startTime: startTime)

        let data = try Data(contentsOf: runsDirectory.appending(path: "\(run.id.uuidString).json"))
        let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        #expect(json["schemaVersion"] as? Int == 1)
        #expect(json["pid"] as? Int == Int(pid))
        #expect(json["serviceID"] as? String == run.serviceID.uuidString)
    }

    @Test func `reads a process's start time`() throws {
        let mine = try #require(ProcessStartTime.of(getpid()))
        let parent = try #require(ProcessStartTime.of(getppid()))
        #expect(parent != mine)
        #expect(ProcessStartTime.of(99_999_999) == nil)
    }

    // MARK: Detecting

    @Test func `a new session finds the previous session's surviving Runs`() async throws {
        let (service, run) = try await startInPreviousSession()

        let orphans = RunRegistry(root: root).orphans()

        #expect(orphans.map(\.runID) == [run.id])
        #expect(orphans.first?.serviceID == service.id)
        #expect(orphans.first?.startedAt == run.startedAt)
        try await previousSession.stop(service.id, origin: .mainWindow)
    }

    @Test func `the current session's own Runs are not Orphans`() async throws {
        let registry = RunRegistry(root: root)
        let manager = ServiceManager(
            library: library, logs: LogStore(root: root), runRegistry: registry,
            baseEnvironment: ["PATH": "/usr/bin:/bin", "SHELL": "/bin/sh"], stopPolicy: quickStop
        )
        let service = try await library.addService(
            toProject: project.id, name: "Own", key: Key("own"), commandLine: "sleep 1000", origin: .mainWindow
        )
        try await manager.start(service.id, origin: .mainWindow)

        #expect(registry.orphans().isEmpty)
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `a record whose process group is gone is not an Orphan, and is removed`() throws {
        let registry = RunRegistry(root: root)
        // A pid that is not running: a process we started and reaped.
        var pid: pid_t = 0
        let argv: [UnsafeMutablePointer<CChar>?] = [strdup("true"), nil]
        defer { free(argv[0]) }
        posix_spawn(&pid, "/usr/bin/true", nil, nil, argv, nil)
        var status: Int32 = 0
        waitpid(pid, &status, 0)
        registry.record(
            Run(id: UUID(), serviceID: UUID(), origin: .mainWindow, startedAt: Date()),
            pid: pid, startTime: ProcessStartTime(seconds: 1, microseconds: 0)
        )

        #expect(RunRegistry(root: root).orphans().isEmpty)
        #expect(recordFiles().isEmpty)
    }

    @Test func `a reused pid with a different start time is not an Orphan, and is removed`() async throws {
        // A live process-group leader that is not the recorded one.
        let tree = try ProcessTree.launch(
            LaunchConfiguration(commandLine: "sleep 1000", workingDirectory: root, environment: [:], shell: "/bin/sh"),
            stopPolicy: quickStop
        )
        let realStart = try #require(ProcessStartTime.of(tree.pid))
        RunRegistry(root: root).record(
            Run(id: UUID(), serviceID: UUID(), origin: .mainWindow, startedAt: Date()),
            pid: tree.pid, startTime: ProcessStartTime(seconds: realStart.seconds - 60, microseconds: 0)
        )

        #expect(RunRegistry(root: root).orphans().isEmpty)
        #expect(recordFiles().isEmpty)
        await tree.stop()
    }

    @Test func `a group whose leader exited is still an Orphan`() async throws {
        // The shell exits; a leftover stays in its group, as if localhost
        // crashed before it could clean up.
        let tree = try ProcessTree.launch(
            LaunchConfiguration(
                commandLine: leftoverIgnoringSignals(thenParent: "exit 0"),
                workingDirectory: root, environment: [:], shell: "/bin/sh"
            ),
            stopPolicy: StopPolicy(interruptTimeout: .seconds(30), terminateTimeout: .seconds(30))
        )
        let startTime = try #require(ProcessStartTime.of(tree.pid))
        let run = Run(id: UUID(), serviceID: UUID(), origin: .mainWindow, startedAt: Date())
        RunRegistry(root: root).record(run, pid: tree.pid, startTime: startTime)
        _ = await tree.waitForShellExit()
        #expect(ProcessStartTime.of(tree.pid) == nil)

        let registry = RunRegistry(root: root)
        let orphans = registry.orphans()
        #expect(orphans.map(\.runID) == [run.id])

        await registry.stopOrphans(orphans, policy: quickStop)
        #expect(!processGroupExists(tree.pid))
    }

    // MARK: Resolving

    @Test func `stopping Orphans kills their whole process trees and forgets them`() async throws {
        let (_, first) = try await startInPreviousSession("sleep 1000 & sleep 1000 & wait")
        let (_, second) = try await startInPreviousSession("trap '' INT TERM; sleep 1000")
        try await Task.sleep(for: .milliseconds(100))

        let registry = RunRegistry(root: root)
        let orphans = registry.orphans()
        #expect(Set(orphans.map(\.runID)) == [first.id, second.id])

        let started = ContinuousClock.now
        await registry.stopOrphans(orphans, policy: quickStop)

        for orphan in orphans {
            #expect(!processGroupExists(orphan.processGroupID))
        }
        #expect(registry.orphans().isEmpty)
        // Stopped in parallel: the SIGKILL-needing one sets the pace.
        #expect(ContinuousClock.now - started < .milliseconds(1500))
    }

    @Test func `leaving Orphans forgets them without touching their processes`() async throws {
        let (service, _) = try await startInPreviousSession()
        let registry = RunRegistry(root: root)
        let orphans = registry.orphans()

        registry.leaveOrphans(orphans)

        #expect(recordFiles().isEmpty)
        #expect(RunRegistry(root: root).orphans().isEmpty)
        #expect(orphans.allSatisfy { processGroupExists($0.processGroupID) })
        try await previousSession.stop(service.id, origin: .mainWindow)
    }

    @Test func `Orphans are never re-adopted: the new session sees their Services as Stopped`() async throws {
        let (service, _) = try await startInPreviousSession()
        let registry = RunRegistry(root: root)
        let newSession = ServiceManager(library: library, logs: LogStore(root: root), runRegistry: registry)

        #expect(registry.orphans().count == 1)
        #expect(await newSession.status(of: service.id).state == .stopped)
        try await previousSession.stop(service.id, origin: .mainWindow)
    }
}
