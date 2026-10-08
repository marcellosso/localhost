import Darwin
import Foundation
import Testing
@testable import LocalhostCore

/// A port nothing listens on right now: bound once by the kernel's choice, then released.
func freePort() -> Int {
    let socket = Darwin.socket(AF_INET, SOCK_STREAM, IPPROTO_TCP)
    defer { close(socket) }
    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_addr = in_addr(s_addr: INADDR_LOOPBACK.bigEndian)
    var length = socklen_t(MemoryLayout<sockaddr_in>.size)
    _ = withUnsafeMutablePointer(to: &address) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            bind(socket, $0, length) == 0 ? getsockname(socket, $0, &length) : -1
        }
    }
    return Int(UInt16(bigEndian: address.sin_port))
}

/// Ports and URLs: readiness, `PORT`, detected and opened URLs.
@Suite(.timeLimit(.minutes(1)))
final class PortTests: Sendable {
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
            portTimeout: .milliseconds(800),
            portProbeInterval: .milliseconds(50),
            stopPolicy: StopPolicy(interruptTimeout: .milliseconds(500), terminateTimeout: .milliseconds(500))
        )
        let projectFolder = root.appending(path: "project")
        try FileManager.default.createDirectory(at: projectFolder, withIntermediateDirectories: true)
        project = try await library.addProject(name: "App", key: Key("app"), path: projectFolder.path, origin: .mainWindow)
    }

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    func service(_ commandLine: String, port: Int? = nil, url: String? = nil) async throws -> Service {
        try await library.addService(
            toProject: project.id, name: "Web", key: Key("web-\(UUID().uuidString.prefix(6).lowercased())"),
            commandLine: commandLine, port: port, url: url, origin: .mainWindow
        )
    }

    func statuses(of serviceID: UUID, _ events: AsyncStream<ServiceEvent>, until isLast: (ServiceStatus) -> Bool) async -> [ServiceStatus] {
        var seen: [ServiceStatus] = []
        for await case .serviceStateChanged(let status) in events where status.serviceID == serviceID {
            seen.append(status)
            if isLast(status) { break }
        }
        return seen
    }

    // MARK: Readiness

    @Test func `with a Port, stays Starting until the Port accepts connections`() async throws {
        let port = freePort()
        let service = try await service("sleep 0.6; exec nc -lk $PORT", port: port)
        let (_, events) = await manager.subscribe()

        let started = ContinuousClock.now
        let status = try await manager.start(service.id, origin: .mainWindow)
        #expect(status.run?.port == port)
        let seen = await statuses(of: service.id, events) { $0.state == .running }

        #expect(ContinuousClock.now - started >= .milliseconds(600))
        #expect(seen.map(\.state) == [.starting, .running])
        #expect(seen.last?.run?.isPortUnresponsive == false)
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `a Port that never opens times out to Running with a warning, cleared once it opens`() async throws {
        let port = freePort()
        let service = try await service("sleep 1.2; exec nc -lk $PORT", port: port)
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)

        let seen = await statuses(of: service.id, events) { $0.state == .running && $0.run?.isPortUnresponsive == false }

        #expect(seen.map(\.state) == [.starting, .running, .running])
        #expect(seen.map { $0.run?.isPortUnresponsive } == [false, true, false])
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `a Service listening on IPv6 loopback only counts as accepting`() async throws {
        let port = freePort()
        let service = try await service("exec nc -6 -lk ::1 $PORT", port: port)
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)

        let seen = await statuses(of: service.id, events) { $0.state == .running }
        #expect(seen.last?.run?.isPortUnresponsive == false)
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `the command gets the Port as PORT, overriding a Service variable`() async throws {
        let marker = root.appending(path: "port.txt")
        let service = try await library.addService(
            toProject: project.id, name: "Web", key: Key("web"),
            commandLine: "echo $PORT > '\(marker.path)'; exec sleep 1000",
            environment: ["PORT": "1"], port: 4321, origin: .mainWindow
        )
        try await manager.start(service.id, origin: .mainWindow)
        try await Task.sleep(for: .milliseconds(300))

        #expect(try String(contentsOf: marker, encoding: .utf8) == "4321\n")
        try await manager.stop(service.id, origin: .mainWindow)
    }

    // MARK: URLs

    @Test func `remembers the first local URL a Run prints`() async throws {
        let service = try await service(#"printf '\033[32mLocal:\033[0m http://0.0.0.0:5173/\n'; echo http://localhost:9999; exec sleep 1000"#)
        let (_, events) = await manager.subscribe()
        try await manager.start(service.id, origin: .mainWindow)

        let seen = await statuses(of: service.id, events) { $0.run?.detectedURL != nil }
        #expect(seen.last?.run?.detectedURL == URL(string: "http://localhost:5173/"))
        #expect(service.urlToOpen(for: seen.last?.run) == URL(string: "http://localhost:5173/"))
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `opens the configured URL, moved to the Run's port, before localhost at the Port`() throws {
        var run = Run(id: UUID(), serviceID: UUID(), origin: .mainWindow, startedAt: Date())
        run.port = 3001
        run.detectedURL = URL(string: "http://localhost:9999")

        let configured = Service(name: "Web", key: try Key("web"), workingDirectory: ".", commandLine: "x",
                                 port: 3000, url: "http://localhost:3000/admin")
        #expect(configured.urlToOpen(for: run) == URL(string: "http://localhost:3001/admin"))

        let otherHost = Service(name: "Web", key: try Key("web"), workingDirectory: ".", commandLine: "x",
                                port: 3000, url: "https://app.test")
        #expect(otherHost.urlToOpen(for: run) == URL(string: "https://app.test"))

        let portOnly = Service(name: "Web", key: try Key("web"), workingDirectory: ".", commandLine: "x", port: 3000)
        #expect(portOnly.urlToOpen(for: run) == URL(string: "http://localhost:3001"))
        #expect(portOnly.urlToOpen(for: nil) == URL(string: "http://localhost:3000"))

        let neither = Service(name: "Web", key: try Key("web"), workingDirectory: ".", commandLine: "x")
        run.port = nil
        #expect(neither.urlToOpen(for: run) == URL(string: "http://localhost:9999"))
        #expect(neither.urlToOpen(for: nil) == nil)
    }

    @Test(arguments: [
        ("  ➜  Local:   http://localhost:5173/", "http://localhost:5173/"),
        ("Uvicorn running on http://0.0.0.0:8000 (Press CTRL+C to quit)", "http://localhost:8000"),
        ("Listening at http://127.0.0.1:3000.", "http://127.0.0.1:3000"),
        ("served on http://[::1]:4000/docs, ok", "http://[::1]:4000/docs"),
        ("bound to http://[::]:4000", "http://localhost:4000"),
        ("see HTTPS://LOCALHOST:8443/a?b=c", "HTTPS://LOCALHOST:8443/a?b=c"),
    ])
    func `detects local URLs in output`(line: String, expected: String) {
        #expect(LocalURL.first(in: line)?.absoluteString == expected)
    }

    @Test func `ignores URLs of other hosts`() {
        #expect(LocalURL.first(in: "docs at https://vitejs.dev and http://example.com:3000") == nil)
        #expect(LocalURL.matches(in: "http://localhost:1 and http://localhost:2").count == 2)
    }

    // MARK: Configuration

    @Test func `validates, stores and clears the Port and URL`() async throws {
        let service = try await service("true", port: 3000, url: " http://localhost:3000/ ")
        #expect(service.port == 3000)
        #expect(service.url == "http://localhost:3000/")

        await #expect(throws: LocalhostError.invalidPort(70_000)) {
            try await self.library.editService(id: service.id, port: 70_000, origin: .mainWindow)
        }
        await #expect(throws: LocalhostError.invalidURL("localhost:3000")) {
            try await self.library.editService(id: service.id, url: "localhost:3000", origin: .mainWindow)
        }

        let cleared = try await library.editService(id: service.id, port: .some(nil), url: "", origin: .mainWindow)
        #expect(cleared.port == nil)
        #expect(cleared.url == nil)
        let reloaded = try ProjectLibrary(storageRoot: root)
        #expect(try await reloaded.service(id: service.id).service == cleared)
    }
}
