import Foundation
import Testing
@testable import LocalhostCore

// The Swift layer over the Rust core. The core's behaviour is tested in
// core/ (cargo test); these cover what the layer adds: labels, Int ports,
// typed errors, streams and Foundation types.

/// A storage root of its own, removed after the test.
private final class TemporaryRoot {
    let url: URL

    /// `isShort` keeps it short enough to hold the shared core's socket.
    init(isShort: Bool = false) throws {
        url = isShort
            ? URL(filePath: "/tmp/localhost-\(UUID().uuidString.prefix(8))", directoryHint: .isDirectory)
            : FileManager.default.temporaryDirectory.appending(path: "localhost-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    }

    deinit {
        try? FileManager.default.removeItem(at: url)
    }
}

@Suite struct ValueTests {
    @Test func validatesKeys() throws {
        #expect(try Key("my-app").rawValue == "my-app")
        #expect(throws: LocalhostError.InvalidKey("My App")) { try Key("My App") }
        #expect(Key(suggestedFrom: "  Café API  ")?.rawValue == "cafe-api")
        #expect(Key(suggestedFrom: "!!!") == nil)
    }

    @Test func errorsCarryTheCoresMessages() {
        #expect(LocalhostError.CommandNotFound.message == "Command not found.")
        #expect(LocalhostError.InvalidPort(70000).message.contains("70000"))
    }

    @Test func findsLocalURLsAfterMultibyteText() throws {
        let text = "✓ ready → http://localhost:3000/app (0.0.0.0)"
        let matches = LocalURL.matches(in: text)
        #expect(matches.count == 1)
        let match = try #require(matches.first)
        #expect(String(text[match.range]) == "http://localhost:3000/app")
        #expect(match.url == URL(string: "http://localhost:3000/app"))
        #expect(LocalURL.first(in: "nothing here") == nil)
    }

    @Test func parsesAnsiAndScoresMatches() {
        let line = ANSI.parse("\u{1B}[1;32mready\u{1B}[0m in 3ms")
        #expect(line.plainText == "ready in 3ms")
        #expect(line.spans.first?.style.isBold == true)
        #expect(ANSI.strip("\u{1B}[31merror\u{1B}[0m") == "error")
        #expect(FuzzyMatch.score("ap", in: "api") != nil)
        #expect(FuzzyMatch.score("zz", in: "api") == nil)
    }

    @Test func roundTripsAProjectFile() throws {
        let yaml = """
        localhost: 1
        name: Shop
        key: shop
        path: /Users/me/shop
        services:
          web:
            name: Web
            command: pnpm dev
            port: 3000
        """
        let document = try ProjectDocument(yaml: yaml)
        #expect(document.name == "Shop")
        #expect(document.path == "/Users/me/shop")
        #expect(document.services.first?.port == 3000)
        let placed = document.withPath("/tmp/shop")
        #expect(placed.path == "/tmp/shop")
        #expect(document.path == "/Users/me/shop")
        #expect(try ProjectDocument(yaml: placed.yaml).path == "/tmp/shop")
        #expect(throws: LocalhostError.self) { try ProjectDocument(yaml: "name: Shop") }
    }

    // Only what reads no agent's files: the rest would read the real home folder.
    @Test func namesAgentsAndKeepsTheirCommand() throws {
        #expect(AgentClient.claudeCode.name == "Claude Code")
        #expect(AgentClient.codex.name == "Codex")
        let root = try TemporaryRoot()
        let command = root.url.appending(path: "Helpers/localhost")
        #expect(AgentSetup(command: command, storageRoot: root.url).command == command)
        #expect(LocalhostError.CommandNotBundled("/x/localhost").message.contains("/x/localhost"))
    }
}

@Suite(.serialized) struct LibraryTests {
    @Test func addsAndEditsProjectsWithIntPorts() async throws {
        let root = try TemporaryRoot()
        let library = try ProjectLibrary(storageRoot: root.url)
        let (projects, events) = library.subscribe()
        #expect(projects.isEmpty)

        let project = try await library.addProject(name: "Shop", key: Key("shop"), path: "/tmp/shop", origin: .mainWindow)
        let service = try await library.addService(
            toProject: project.id, name: "Web", key: Key("web"), commandLine: "pnpm dev", port: 3000, origin: .mainWindow
        )
        #expect(service.port == 3000)

        let edited = try await library.editService(id: service.id, port: .some(nil), origin: .menuBar)
        #expect(edited.port == nil)

        var iterator = events.makeAsyncIterator()
        guard case .projectAdded(let added, .mainWindow) = await iterator.next() else {
            Issue.record("Expected projectAdded first")
            return
        }
        #expect(added.id == project.id)

        await #expect(throws: LocalhostError.DuplicateProjectKey(try Key("shop"))) {
            try await library.addProject(name: "Other", key: Key("shop"), path: "/tmp/other", origin: .mainWindow)
        }
        #expect(throws: LocalhostError.ProjectNotFound(service.id)) { try library.project(id: service.id) }

        // A second library reads what the first saved.
        let reopened = try ProjectLibrary(storageRoot: root.url)
        #expect(reopened.projects().map(\.name) == ["Shop"])
        #expect(reopened.unreadableFiles.isEmpty)
    }

    @Test func suggestsKeysTheProjectDoesNotUse() async throws {
        let root = try TemporaryRoot()
        let library = try ProjectLibrary(storageRoot: root.url)
        let project = try await library.addProject(name: "Shop", key: Key("shop"), path: "/tmp/shop", origin: .mainWindow)
        let service = try await library.addService(
            toProject: project.id, name: "Web", key: Key("web"), commandLine: "pnpm dev", origin: .mainWindow
        )
        let projects = library.projects()
        #expect(Project.suggestedKey(for: "Shop", among: projects).rawValue == "shop-2")
        #expect(Project.suggestedKey(for: "Shop", among: projects, excluding: project.id).rawValue == "shop")
        let reloaded = try library.project(id: project.id)
        #expect(reloaded.suggestedServiceKey(for: "Web").rawValue == "web-2")
        #expect(reloaded.suggestedServiceKey(for: "Web", excluding: service.id).rawValue == "web")
        #expect(ServiceState.mostSignificant(of: [.running, .failed]) == .failed)
        #expect(ServiceState.mostSignificant(of: []) == nil)
    }
}

@Suite(.serialized) struct RunTests {
    @Test @MainActor func runsAServiceAndStreamsItsOutput() async throws {
        let root = try TemporaryRoot()
        let library = try ProjectLibrary(storageRoot: root.url)
        let logs = LogStore(root: root.url)
        let manager = ServiceManager(library: library, logs: logs, runRegistry: RunRegistry(root: root.url))
        let store = ServiceStatusStore(manager: manager)

        let project = try await library.addProject(
            name: "Echo", key: Key("echo"), path: root.url.path(percentEncoded: false), origin: .mainWindow
        )
        let service = try await library.addService(
            toProject: project.id, name: "Echo", key: Key("echo"),
            commandLine: "echo on $PORT; exec sleep 30", port: 45_678, portCanChange: false, origin: .mainWindow
        )

        let (_, updates) = logs.subscribe(toServices: [service.id])
        let started = try await manager.start(service.id, origin: .mainWindow)
        #expect(started.state.isActive)

        var lines = updates.makeAsyncIterator()
        var printed: [String] = []
        while let line = await lines.next() {
            if line.line.channel == .stdout {
                printed.append(line.line.text)
                break
            }
        }
        #expect(printed == ["on 45678"])
        #expect(manager.status(of: service.id).run?.port == 45_678)

        let first = try #require(try await manager.runStats(for: service.id))
        #expect(first.runId == started.run?.id)
        #expect(first.processCount >= 1)
        #expect(first.cpuPercent == nil)
        let second = try await manager.runStats(for: service.id, previous: first)
        #expect(second?.cpuPercent != nil)

        let stopped = try await manager.stop(service.id, origin: .mainWindow)
        #expect(stopped.state == .stopped)
        #expect(stopped.run?.termination?.summary.isEmpty == false)
        #expect(try await manager.runStats(for: service.id) == nil)

        // The store follows the manager's events on the main actor.
        for _ in 0..<50 where store.status(of: service.id).state != .stopped {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(store.status(of: service.id).state == .stopped)
        let unknown = UUID()
        #expect(store.status(of: unknown) == .neverRun(unknown))
        #expect(manager.status(of: service.id).run?.serviceID == service.id)
    }
}

@Suite(.serialized) struct SharedCoreTests {
    /// The app's path: `Targets` on a shared core, here served by this
    /// process, since a test can't be started as one.
    @Test @MainActor func runsAServiceThroughTheSharedCore() async throws {
        let root = try TemporaryRoot(isShort: true)
        let url = root.url
        Thread.detachNewThread { _ = runSharedCore(storageRoot: url) }
        // Up before connecting, or `Targets` would start this test's executable as the core.
        let socket = url.appending(path: "localhostd.sock").path(percentEncoded: false)
        for _ in 0..<200 where !FileManager.default.fileExists(atPath: socket) {
            try await Task.sleep(for: .milliseconds(25))
        }
        let library = try ProjectLibrary(storageRoot: url)
        let targets = Targets(library: library, storageRoot: url)
        let store = ServiceStatusStore(manager: targets)
        try await targets.connect()

        let project = try await library.addProject(
            name: "Echo", key: Key("echo"), path: url.path(percentEncoded: false), origin: .mainWindow
        )
        let service = try await library.addService(
            toProject: project.id, name: "Echo", key: Key("echo"),
            commandLine: "echo on $PORT; exec sleep 30", port: 45_679, portCanChange: false, origin: .mainWindow
        )
        let started = try await targets.start(service.id, origin: .mainWindow)
        #expect(started.state.isActive)
        let runID = try #require(started.run?.id)

        let (buffered, updates) = try await targets.subscribe(toRun: runID)
        var printed = buffered.filter { $0.channel == .stdout }.map(\.text)
        var lines = updates.makeAsyncIterator()
        while printed.isEmpty, let line = await lines.next() {
            if line.channel == .stdout { printed.append(line.text) }
        }
        #expect(printed == ["on 45679"])

        let stopped = try await targets.stop(service.id, origin: .mainWindow)
        #expect(stopped.state == .stopped)
        for _ in 0..<50 where store.status(of: service.id).state != .stopped {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(store.status(of: service.id).state == .stopped)
        try await targets.removeLogs(ofService: service.id)
    }
}
