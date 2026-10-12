import Darwin
import Foundation
import Testing
@testable import LocalhostCore

/// Lifecycle edge cases against real processes: storms of starts and stops,
/// restarts during Starting, output floods, huge lines and bytes that aren't
/// UTF-8. Each command line ends with a unique marker so a test can check
/// that nothing it launched is left running.
@Suite(.timeLimit(.minutes(1)))
final class HardeningTests: Sendable {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let library: ProjectLibrary
    /// Small, so the bounds are reached quickly.
    let logs: LogStore
    let manager: ServiceManager
    let project: Project
    let marker = "hardening-\(UUID().uuidString)"

    init() async throws {
        library = try ProjectLibrary(storageRoot: root)
        logs = LogStore(root: root, maxLinesPerRun: 1_000, maxFileSize: 1_000_000)
        manager = ServiceManager(
            library: library, logs: logs, runRegistry: RunRegistry(root: root),
            baseEnvironment: ["PATH": "/usr/bin:/bin", "HOME": NSHomeDirectory(), "SHELL": "/bin/sh"],
            // Long enough that a command ending on its own does so within Starting on a busy machine.
            gracePeriod: .seconds(5),
            stopPolicy: StopPolicy(interruptTimeout: .milliseconds(500), terminateTimeout: .milliseconds(500))
        )
        let folder = root.appending(path: "project")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        project = try await library.addProject(name: "App", key: Key("app"), path: folder.path, origin: .mainWindow)
    }

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    /// A Service running `commandLine`, tagged with the test's marker.
    func service(_ commandLine: String, port: Int? = nil, dependencies: [UUID] = []) async throws -> Service {
        try await library.addService(
            toProject: project.id, name: "Service", key: Key("svc-\(UUID().uuidString.prefix(8).lowercased())"),
            commandLine: "\(commandLine); : \(marker)", dependencies: dependencies,
            port: port, portCanChange: false, origin: .mainWindow
        )
    }

    /// The pids of processes this test launched that are still alive.
    func leftovers() throws -> [String] {
        let pgrep = Process()
        pgrep.executableURL = URL(filePath: "/usr/bin/pgrep")
        pgrep.arguments = ["-f", marker]
        let output = Pipe()
        pgrep.standardOutput = output
        try pgrep.run()
        pgrep.waitUntilExit()
        let text = String(decoding: output.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        return text.split(separator: "\n").map(String.init)
    }

    /// The Run records left in the registry (an Orphan candidate each).
    func runRecords() -> [String] {
        (try? FileManager.default.contentsOfDirectory(atPath: root.appending(path: "runs").path)) ?? []
    }

    /// The most Runs of `serviceID` that were active at once, as seen in its
    /// events, once `body` has run and the events are drained.
    func maxActiveRuns(of serviceID: UUID, during body: () async throws -> Void) async throws -> Int {
        let (_, events) = await manager.subscribe()
        let watcher = Task {
            var active: Set<UUID> = []
            var maxActive = 0
            for await event in events where event.serviceID == serviceID {
                switch event {
                case .runStarted(let run): active.insert(run.id)
                case .runEnded(let run): active.remove(run.id)
                case .serviceStateChanged: break
                }
                maxActive = max(maxActive, active.count)
            }
            return maxActive
        }
        try await body()
        try await Task.sleep(for: .milliseconds(200))
        watcher.cancel()
        return await watcher.value
    }

    func logFileSize(ofRun runID: UUID) async throws -> Int {
        let url = try #require(await logs.logFileURL(forRun: runID))
        let sizes = [url, url.appendingPathExtension("1")].map {
            (try? FileManager.default.attributesOfItem(atPath: $0.path)[.size] as? Int) ?? 0
        }
        return sizes.reduce(0, +)
    }

    // MARK: Rapid start and stop

    @Test func `a storm of concurrent starts, stops and restarts runs one Run at a time and leaves nothing behind`() async throws {
        let service = try await service("sleep 1000")

        let maxActive = try await maxActiveRuns(of: service.id) {
            await withDiscardingTaskGroup { group in
                for index in 0..<45 {
                    group.addTask { [manager] in
                        switch index % 3 {
                        case 0: _ = try? await manager.start(service.id, origin: .mainWindow)
                        case 1: _ = try? await manager.stop(service.id, origin: .menuBar)
                        default: _ = try? await manager.restart(service.id, origin: .mainWindow)
                        }
                    }
                }
            }
            try await manager.stop(service.id, origin: .mainWindow)
        }

        #expect(maxActive <= 1)
        #expect(await manager.status(of: service.id).state == .stopped)
        #expect(try leftovers().isEmpty)
        #expect(runRecords().isEmpty)
    }

    @Test func `starting and stopping at once, again and again, always ends Stopped`() async throws {
        let service = try await service("sleep 1000")

        for _ in 0..<20 {
            try await manager.start(service.id, origin: .mainWindow)
            let status = try await manager.stop(service.id, origin: .mainWindow)
            #expect(status.state == .stopped)
        }

        #expect(try leftovers().isEmpty)
        #expect(runRecords().isEmpty)
    }

    // MARK: Restart during Starting

    @Test func `restarting during Starting replaces the Run and its whole tree`() async throws {
        // The Port never opens, so the Run stays Starting.
        let service = try await service("sleep 1000", port: freePort())

        let first = try await manager.start(service.id, origin: .mainWindow)
        let second = try await manager.restart(service.id, origin: .mainWindow)

        #expect(first.state == .starting)
        #expect(second.state == .starting)
        #expect(second.run?.id != first.run?.id)
        // Only the new Run's shell is alive.
        #expect(try leftovers().count == 1)

        try await manager.stop(service.id, origin: .mainWindow)
        #expect(try leftovers().isEmpty)
        #expect(runRecords().isEmpty)
    }

    @Test func `restarting a Service still waiting for its Dependencies, then stopping all, leaves nothing behind`() async throws {
        let database = try await service("sleep 1000", port: freePort())
        let api = try await service("sleep 1000", dependencies: [database.id])

        // `start` returns once the Dependency is satisfied, which takes the port timeout here.
        let start = Task { [manager] in try await manager.start(api.id, origin: .mainWindow) }
        while await manager.status(of: database.id).run == nil { try await Task.sleep(for: .milliseconds(20)) }
        let restart = Task { [manager] in try await manager.restart(api.id, origin: .mainWindow) }
        try await Task.sleep(for: .milliseconds(200))
        let stopping = ContinuousClock.now
        try await manager.stopAll(inProject: project.id, origin: .mainWindow)
        _ = try await (start.value, restart.value)
        try await manager.stopAll(inProject: project.id, origin: .mainWindow)

        // Nothing waits for the Port that never opens.
        #expect(ContinuousClock.now - stopping < .seconds(10))
        #expect(await manager.status(of: api.id).state == .stopped)
        #expect(await manager.status(of: database.id).state == .stopped)
        #expect(try leftovers().isEmpty)
        #expect(runRecords().isEmpty)
    }

    // MARK: Output

    @Test func `a command flooding its output stays bounded and still stops promptly`() async throws {
        let service = try await service("yes 'a chatty line of output'")

        let status = try await manager.start(service.id, origin: .mainWindow)
        let runID = try #require(status.run?.id)
        try await Task.sleep(for: .seconds(1))
        let started = ContinuousClock.now
        let stopped = try await manager.stop(service.id, origin: .mainWindow)

        #expect(stopped.state == .stopped)
        #expect(ContinuousClock.now - started < .seconds(5))
        #expect(stopped.run?.wasForceKilled == false)
        #expect(await logs.lines(forRun: runID).count == 1_000)
        #expect(try await logFileSize(ofRun: runID) <= 1_000_000)
        #expect(try leftovers().isEmpty)
    }

    @Test func `a line of megabytes is kept in pieces, and its log file stays within the cap`() async throws {
        let service = try await service(#"head -c 3000000 /dev/zero | tr '\0' x; echo"#)

        let status = try await manager.start(service.id, origin: .mainWindow)
        let runID = try #require(status.run?.id)
        let (_, updates) = await logs.subscribe(toRun: runID)
        for await _ in updates {}

        let pieces = await logs.lines(forRun: runID).filter { $0.channel == .stdout }
        #expect(pieces.allSatisfy { $0.text.utf8.count <= PipeReader.maxLineLength })
        #expect(pieces.map(\.text).joined() == String(repeating: "x", count: 3_000_000))
        #expect(try await logFileSize(ofRun: runID) <= 1_000_000)
    }

    @Test func `output that isn't UTF-8 is shown with replacement characters and still read for URLs`() async throws {
        let service = try await service(#"printf 'caf\351 on http://localhost:4321/\nnul\000byte\n'; sleep 1000"#)

        let status = try await manager.start(service.id, origin: .mainWindow)
        let runID = try #require(status.run?.id)
        let (_, events) = await manager.subscribe()
        for await event in events {
            if case .serviceStateChanged(let status) = event, status.serviceID == service.id, status.run?.detectedURL != nil { break }
        }

        #expect(await manager.status(of: service.id).run?.detectedURL == URL(string: "http://localhost:4321/"))
        #expect(await logs.lines(forRun: runID).map(\.text) == ["caf\u{FFFD} on http://localhost:4321/", "nul\0byte"])
        try await manager.stop(service.id, origin: .mainWindow)
        // The log file is valid UTF-8 text.
        let file = try #require(await logs.logFileURL(forRun: runID))
        #expect(try String(contentsOf: file, encoding: .utf8).contains("caf\u{FFFD} on"))
    }
}
