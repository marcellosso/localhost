import Darwin
import Foundation
import Testing
@testable import LocalhostCore

struct LogBufferTests {
    func line(_ text: String) -> OutputLine {
        OutputLine(channel: .stdout, text: text, timestamp: Date())
    }

    @Test func `keeps lines in order until full`() {
        var buffer = LogBuffer(capacity: 3)
        buffer.append(line("a"))
        buffer.append(line("b"))
        #expect(buffer.lines.map(\.text) == ["a", "b"])
        #expect(buffer.droppedCount == 0)
    }

    @Test func `drops the oldest lines once full`() {
        var buffer = LogBuffer(capacity: 3)
        for text in ["a", "b", "c", "d", "e"] { buffer.append(line(text)) }
        #expect(buffer.lines.map(\.text) == ["c", "d", "e"])
        #expect(buffer.droppedCount == 2)
    }

    @Test func `recent lines are bounded by the limit`() {
        var buffer = LogBuffer(capacity: 10)
        for text in ["a", "b", "c", "d"] { buffer.append(line(text)) }
        #expect(buffer.recentLines(2).map(\.text) == ["c", "d"])
        #expect(buffer.recentLines(100).map(\.text) == ["a", "b", "c", "d"])
    }
}

final class LogStoreTests: Sendable {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    func makeRun(service serviceID: UUID = UUID()) -> Run {
        Run(id: UUID(), serviceID: serviceID, origin: .mainWindow, startedAt: Date())
    }

    func line(_ text: String, _ channel: OutputLine.Channel = .stdout) -> OutputLine {
        OutputLine(channel: channel, text: text, timestamp: Date(timeIntervalSince1970: 1_800_000_000.25))
    }

    func logDirectory(_ serviceID: UUID) -> URL {
        root.appending(path: "logs/\(serviceID.uuidString)")
    }

    func fileContents(_ url: URL) throws -> String {
        try String(contentsOf: url, encoding: .utf8)
    }

    @Test func `writes each Run to logs, service, run-id dot log`() async throws {
        let store = LogStore(root: root)
        let run = makeRun()
        await store.beginRun(run)
        await store.append(line("hello"), toRun: run.id)
        await store.append(line("oops", .stderr), toRun: run.id)
        await store.append(line("Exited with code 0", .localhost), toRun: run.id)
        await store.endRun(run.id)

        let file = logDirectory(run.serviceID).appending(path: "\(run.id.uuidString).log")
        #expect(await store.logFileURL(forRun: run.id) == file)
        #expect(try fileContents(file) == """
        2027-01-15T08:00:00.250Z stdout hello
        2027-01-15T08:00:00.250Z stderr oops
        2027-01-15T08:00:00.250Z localhost Exited with code 0

        """)
    }

    @Test func `keeps lines in memory with their channel and timestamp`() async throws {
        let store = LogStore(root: root)
        let run = makeRun()
        await store.beginRun(run)
        await store.append(line("a"), toRun: run.id)
        await store.append(line("b", .stderr), toRun: run.id)

        #expect(await store.lines(forRun: run.id) == [line("a"), line("b", .stderr)])
    }

    @Test func `the in-memory buffer is bounded`() async throws {
        let store = LogStore(root: root, maxLinesPerRun: 2)
        let run = makeRun()
        await store.beginRun(run)
        for text in ["a", "b", "c"] { await store.append(line(text), toRun: run.id) }

        #expect(await store.lines(forRun: run.id).map(\.text) == ["b", "c"])
        // The file keeps everything.
        let file = try #require(await store.logFileURL(forRun: run.id))
        #expect(try fileContents(file).split(separator: "\n").count == 3)
    }

    @Test func `keeps only the latest Run of each Service in memory`() async throws {
        let store = LogStore(root: root)
        let serviceID = UUID()
        let first = makeRun(service: serviceID)
        await store.beginRun(first)
        await store.append(line("first"), toRun: first.id)
        await store.endRun(first.id)

        let second = makeRun(service: serviceID)
        await store.beginRun(second)

        #expect(await store.lines(forRun: first.id).isEmpty)
        #expect(await store.lines(forRun: second.id).isEmpty)
        #expect(await store.latestRunID(ofService: serviceID) == second.id)
    }

    @Test func `keeps the files of the last 5 Runs per Service`() async throws {
        let store = LogStore(root: root)
        let serviceID = UUID()
        var runs: [Run] = []
        for index in 0..<7 {
            let run = makeRun(service: serviceID)
            runs.append(run)
            await store.beginRun(run)
            await store.append(line("run \(index)"), toRun: run.id)
            await store.endRun(run.id)
        }

        let remaining = try FileManager.default.contentsOfDirectory(atPath: logDirectory(serviceID).path).sorted()
        #expect(remaining == runs.suffix(5).map { "\($0.id.uuidString).log" }.sorted())
    }

    @Test func `removing a Service's logs deletes its files and buffer, and no other Service's`() async throws {
        let store = LogStore(root: root)
        let (removed, kept) = (makeRun(), makeRun())
        for run in [removed, kept] {
            await store.beginRun(run)
            await store.append(line("output"), toRun: run.id)
            await store.endRun(run.id)
        }

        await store.removeLogs(ofService: removed.serviceID)

        #expect(!FileManager.default.fileExists(atPath: logDirectory(removed.serviceID).path))
        #expect(await store.lines(forRun: removed.id).isEmpty)
        #expect(await store.latestRunID(ofService: removed.serviceID) == nil)
        #expect(FileManager.default.fileExists(atPath: logDirectory(kept.serviceID).path))
        #expect(await store.lines(forRun: kept.id).map(\.text) == ["output"])
    }

    @Test func `rotates a Run's file so it stays within the size cap`() async throws {
        let store = LogStore(root: root, maxFileSize: 1_000)
        let run = makeRun()
        await store.beginRun(run)
        for index in 0..<100 {
            await store.append(line("line \(index) " + String(repeating: "x", count: 20)), toRun: run.id)
        }
        await store.endRun(run.id)

        let file = try #require(await store.logFileURL(forRun: run.id))
        let backup = file.appendingPathExtension("1")
        let size = { (url: URL) in try FileManager.default.attributesOfItem(atPath: url.path)[.size] as! Int }
        #expect(try size(file) + size(backup) <= 1_000)
        // The most recent output is kept.
        #expect(try fileContents(file).hasSuffix("line 99 \(String(repeating: "x", count: 20))\n"))
    }

    @Test func `subscribing returns the lines so far, then live lines until the Run ends`() async throws {
        let store = LogStore(root: root)
        let run = makeRun()
        await store.beginRun(run)
        await store.append(line("before"), toRun: run.id)

        let (lines, updates) = await store.subscribe(toRun: run.id)
        await store.append(line("after"), toRun: run.id)
        await store.endRun(run.id)

        #expect(lines.map(\.text) == ["before"])
        var live: [String] = []
        for await line in updates { live.append(line.text) }
        #expect(live == ["after"])
    }

    @Test func `the combined stream merges Services by time and follows their next Runs`() async throws {
        let store = LogStore(root: root)
        let (web, api, other) = (UUID(), UUID(), UUID())
        let webRun = makeRun(service: web)
        let apiRun = makeRun(service: api)
        let otherRun = makeRun(service: other)
        for run in [webRun, apiRun, otherRun] { await store.beginRun(run) }
        let at = { (seconds: Double) in Date(timeIntervalSince1970: seconds) }
        await store.append(OutputLine(channel: .stdout, text: "web 1", timestamp: at(1)), toRun: webRun.id)
        await store.append(OutputLine(channel: .stdout, text: "web 3", timestamp: at(3)), toRun: webRun.id)
        await store.append(OutputLine(channel: .stderr, text: "api 2", timestamp: at(2)), toRun: apiRun.id)
        await store.append(OutputLine(channel: .stdout, text: "other", timestamp: at(2)), toRun: otherRun.id)

        let (lines, updates) = await store.subscribe(toServices: [web, api])
        #expect(lines.map(\.line.text) == ["web 1", "api 2", "web 3"])
        #expect(lines.map(\.serviceID) == [web, api, web])

        await store.append(line("api live"), toRun: apiRun.id)
        await store.append(line("other live"), toRun: otherRun.id)
        await store.endRun(webRun.id)
        let nextWebRun = makeRun(service: web)
        await store.beginRun(nextWebRun)
        await store.append(line("web again"), toRun: nextWebRun.id)

        var live: [ServiceOutputLine] = []
        for await line in updates {
            live.append(line)
            if live.count == 2 { break }
        }
        #expect(live.map(\.line.text) == ["api live", "web again"])
        #expect(live.map(\.serviceID) == [api, web])
    }

    @Test func `clearing a Service's buffer keeps its log file and new output`() async throws {
        let store = LogStore(root: root)
        let run = makeRun()
        await store.beginRun(run)
        await store.append(line("old"), toRun: run.id)
        await store.clearBuffer(ofService: run.serviceID)
        await store.append(line("new"), toRun: run.id)

        #expect(await store.lines(forRun: run.id).map(\.text) == ["new"])
        let file = try #require(await store.logFileURL(forRun: run.id))
        #expect(try fileContents(file).split(separator: "\n").count == 2)
    }

    @Test func `recent output leaves out localhost's own lines`() async throws {
        let store = LogStore(root: root)
        let run = makeRun()
        await store.beginRun(run)
        for text in ["a", "b", "c"] { await store.append(line(text), toRun: run.id) }
        await store.append(line("d", .stderr), toRun: run.id)
        await store.append(line("Exited with code 1", .localhost), toRun: run.id)

        #expect(await store.recentOutput(forRun: run.id, limit: 3).map(\.text) == ["b", "c", "d"])
    }

    @Test func `subscribing to an ended Run returns its lines and a finished stream`() async throws {
        let store = LogStore(root: root)
        let run = makeRun()
        await store.beginRun(run)
        await store.append(line("only"), toRun: run.id)
        await store.endRun(run.id)

        let (lines, updates) = await store.subscribe(toRun: run.id)
        #expect(lines.map(\.text) == ["only"])
        var live: [OutputLine] = []
        for await line in updates { live.append(line) }
        #expect(live.isEmpty)
    }
}

/// Logs written by `ServiceManager` for real Runs.
@Suite(.timeLimit(.minutes(1)))
final class ServiceLogTests: Sendable {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let library: ProjectLibrary
    let logs: LogStore
    let manager: ServiceManager
    let project: Project

    init() async throws {
        library = try ProjectLibrary(storageRoot: root)
        logs = LogStore(root: root)
        manager = ServiceManager(
            library: library,
            logs: logs,
            runRegistry: RunRegistry(root: root),
            baseEnvironment: ["PATH": "/usr/bin:/bin", "HOME": NSHomeDirectory(), "SHELL": "/bin/sh"],
            gracePeriod: .milliseconds(200),
            stopPolicy: StopPolicy(interruptTimeout: .milliseconds(300), terminateTimeout: .milliseconds(300))
        )
        project = try await library.addProject(name: "App", key: Key("app"), path: root.path, origin: .mainWindow)
    }

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    /// Starts a Service and returns its Run's lines once the Run has ended
    /// (stopping it after `stopAfter`, if given).
    func runLines(_ commandLine: String, stopAfter delay: Duration? = nil) async throws -> [OutputLine] {
        let service = try await library.addService(
            toProject: project.id, name: "Svc", key: Key("svc"), commandLine: commandLine, origin: .mainWindow
        )
        let status = try await manager.start(service.id, origin: .mainWindow)
        let runID = try #require(status.run?.id)
        let (_, updates) = await logs.subscribe(toRun: runID)
        if let delay {
            try await Task.sleep(for: delay)
            try await manager.stop(service.id, origin: .mainWindow)
        }
        for await _ in updates {}
        return await logs.lines(forRun: runID)
    }

    @Test func `a Run's output and exit are logged`() async throws {
        let lines = try await runLines("echo out; echo err >&2; sleep 0.3; exit 2")

        #expect(lines.filter { $0.channel == .stdout }.map(\.text) == ["out"])
        #expect(lines.filter { $0.channel == .stderr }.map(\.text) == ["err"])
        #expect(lines.last?.channel == .localhost)
        #expect(lines.last?.text == "Exited with code 2")
    }

    @Test func `a force kill is logged`() async throws {
        let lines = try await runLines("trap '' INT TERM; echo ready; sleep 1000 & wait", stopAfter: .milliseconds(300))
        let notes = lines.filter { $0.channel == .localhost }.map(\.text)

        #expect(notes == [
            "Force-killed: the process group did not stop in time",
            "Terminated by signal SIGKILL",
        ])
    }

    /// The Service's status once its Run has fully ended.
    func endedStatus() async throws -> ServiceStatus {
        let service = try #require(await library.project(id: project.id).services.first)
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline {
            let status = await manager.status(of: service.id)
            if status.run?.endedAt != nil { return status }
            try await Task.sleep(for: .milliseconds(20))
        }
        Issue.record("the Run did not end")
        return await manager.status(of: service.id)
    }

    @Test(arguments: [
        "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
        "failed to connect to the docker API at unix:///Users/me/.docker/run/docker.sock; check if the path is correct and if the daemon is running",
    ])
    func `a Run that fails after Docker's daemon error is dockerDaemonUnavailable`(error: String) async throws {
        _ = try await runLines("echo '\(error)' >&2; exit 1")
        let status = try await endedStatus()

        #expect(status.state == .failed)
        #expect(status.run?.failure == .dockerDaemonUnavailable)
        #expect(status.run?.termination == .exited(code: 1))
    }

    @Test func `Docker's daemon error is not a failure when the Run succeeds`() async throws {
        _ = try await runLines("echo 'Cannot connect to the Docker daemon' >&2; exit 0")
        let status = try await endedStatus()

        #expect(status.state == .exited)
        #expect(status.run?.failure == nil)
    }

    @Test func `the Run's file is written`() async throws {
        let lines = try await runLines("echo persisted")
        #expect(lines.first?.text == "persisted")
        let service = try #require(await library.project(id: project.id).services.first)
        let runID = try #require(await manager.status(of: service.id).run?.id)
        let file = try #require(await logs.logFileURL(forRun: runID))
        let contents = try String(contentsOf: file, encoding: .utf8)
        #expect(contents.contains(" stdout persisted\n"))
    }
}
