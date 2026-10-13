import Foundation
import Testing
@testable import LocalhostCore

// The Swift layer over the Rust core. The core's behaviour is tested in
// core/ (cargo test); these cover what the layer adds: labels, Int ports,
// typed errors, streams and Foundation types.

/// A storage root of its own, removed after the test.
private final class TemporaryRoot {
    let url = FileManager.default.temporaryDirectory.appending(path: "localhost-tests-\(UUID().uuidString)")

    init() throws {
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

        let stopped = try await manager.stop(service.id, origin: .mainWindow)
        #expect(stopped.state == .stopped)
        #expect(stopped.run?.termination?.summary.isEmpty == false)

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
