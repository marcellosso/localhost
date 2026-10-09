import Foundation
import Testing
@testable import LocalhostCore

final class ProjectLibraryTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    func makeLibrary() throws -> ProjectLibrary {
        try ProjectLibrary(storageRoot: root)
    }

    func projectFile(_ id: UUID) -> URL {
        root.appending(path: "projects/\(id.uuidString).json")
    }

    // MARK: Projects

    @Test func `starts empty`() async throws {
        #expect(try await makeLibrary().projects().isEmpty)
    }

    @Test func `adds a project and persists it as its own JSON file`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(
            name: "  My SaaS ", key: Key("my-saas"), path: "/Users/dev/Code/my-saas/", origin: .mainWindow
        )

        #expect(project.name == "My SaaS")
        #expect(project.path == "/Users/dev/Code/my-saas")
        #expect(project.services.isEmpty)
        #expect(!project.isFavorite)

        let json = try JSONSerialization.jsonObject(with: Data(contentsOf: projectFile(project.id))) as? [String: Any]
        #expect(json?["schemaVersion"] as? Int == 1)
    }

    @Test func `reloads exactly what was saved`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        try await library.addService(
            toProject: project.id, name: "Web", key: Key("web"), workingDirectory: "web",
            commandLine: "pnpm dev", variables: VariableInput.plain(["PORT": "3000"]), origin: .mainWindow
        )
        try await library.editProject(id: project.id, isFavorite: true, origin: .menuBar)

        let reloaded = try makeLibrary()
        #expect(await reloaded.projects() == library.projects())
    }

    @Test func `adds a project together with its reviewed services in one save`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(
            name: "Shop", key: Key("shop"), path: "/tmp/shop",
            services: [
                NewService(name: "Web", key: Key("web"), workingDirectory: "./apps/web/", commandLine: "pnpm run dev --port $PORT", port: 5173),
                NewService(name: "Postgres", key: Key("postgres"), commandLine: "docker compose up postgres", port: 5432, portCanChange: false),
            ],
            origin: .mainWindow
        )

        #expect(project.services.map(\.key.rawValue) == ["web", "postgres"])
        #expect(project.services[0].workingDirectory == "apps/web")
        #expect(project.services[1].port == 5432)
        #expect(!project.services[1].portCanChange)
        #expect(try await makeLibrary().projects() == [project])
    }

    @Test func `a project whose services are invalid is not added at all`() async throws {
        let library = try makeLibrary()
        await #expect(throws: LocalhostError.self) {
            try await library.addProject(
                name: "Shop", key: Key("shop"), path: "/tmp/shop",
                services: [
                    NewService(name: "Web", key: Key("web"), commandLine: "pnpm dev"),
                    NewService(name: "Web again", key: Key("web"), commandLine: "pnpm start"),
                ],
                origin: .mainWindow
            )
        }
        #expect(await library.projects().isEmpty)
        #expect(try await makeLibrary().projects().isEmpty)
    }

    @Test func `adds several services at once, all or none`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        try await library.addService(toProject: project.id, name: "API", key: Key("api"), commandLine: "make api", origin: .mainWindow)

        await #expect(throws: LocalhostError.duplicateServiceKey(try Key("api"), projectID: project.id)) {
            try await library.addServices(
                [NewService(name: "Web", key: Key("web"), commandLine: "pnpm dev"),
                 NewService(name: "API", key: Key("api"), commandLine: "uv run api")],
                toProject: project.id, origin: .mainWindow
            )
        }
        #expect(try await library.project(id: project.id).services.count == 1)

        let added = try await library.addServices(
            [NewService(name: "Web", key: Key("web"), commandLine: "pnpm dev")], toProject: project.id, origin: .mainWindow
        )
        #expect(added.map(\.key.rawValue) == ["web"])
        #expect(try await library.project(id: project.id).services.map(\.key.rawValue) == ["api", "web"])
    }

    @Test func `assigns increasing sort orders and lists projects in that order`() async throws {
        let library = try makeLibrary()
        let b = try await library.addProject(name: "B", key: Key("b"), path: "/tmp/b", origin: .mainWindow)
        let a = try await library.addProject(name: "A", key: Key("a"), path: "/tmp/a", origin: .mainWindow)

        #expect(b.sortOrder < a.sortOrder)
        #expect(await library.projects().map(\.id) == [b.id, a.id])
    }

    @Test func `expands a tilde in the project path`() async throws {
        let project = try await makeLibrary().addProject(name: "A", key: Key("a"), path: "~/Code/a", origin: .mainWindow)
        #expect(project.path == NSHomeDirectory() + "/Code/a")
    }

    @Test func `rejects a relative project path`() async throws {
        let library = try makeLibrary()
        await #expect(throws: LocalhostError.invalidProjectPath("Code/a")) {
            try await library.addProject(name: "A", key: Key("a"), path: "Code/a", origin: .mainWindow)
        }
    }

    @Test func `rejects an empty name`() async throws {
        let library = try makeLibrary()
        await #expect(throws: LocalhostError.invalidName) {
            try await library.addProject(name: "  ", key: Key("a"), path: "/tmp/a", origin: .mainWindow)
        }
    }

    @Test func `project keys are unique app-wide`() async throws {
        let library = try makeLibrary()
        try await library.addProject(name: "One", key: Key("app"), path: "/tmp/one", origin: .mainWindow)
        let two = try await library.addProject(name: "Two", key: Key("two"), path: "/tmp/two", origin: .mainWindow)

        await #expect(throws: LocalhostError.duplicateProjectKey(try Key("app"))) {
            try await library.addProject(name: "Three", key: Key("app"), path: "/tmp/three", origin: .mainWindow)
        }
        await #expect(throws: LocalhostError.duplicateProjectKey(try Key("app"))) {
            try await library.editProject(id: two.id, key: Key("app"), origin: .mainWindow)
        }
    }

    @Test func `editing a project can keep its own key`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        let edited = try await library.editProject(id: project.id, name: "Renamed", key: Key("app"), origin: .mainWindow)

        #expect(edited.name == "Renamed")
        #expect(edited.key == project.key)
    }

    @Test func `edits only the fields that are given`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        let edited = try await library.editProject(id: project.id, path: "/tmp/moved", origin: .mainWindow)

        #expect(edited.path == "/tmp/moved")
        #expect(edited.name == "App")
        #expect(edited.key == project.key)
        #expect(try await makeLibrary().project(id: project.id) == edited)
    }

    @Test func `removes a project and its file`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        try await library.removeProject(id: project.id, origin: .mainWindow)

        #expect(await library.projects().isEmpty)
        #expect(!FileManager.default.fileExists(atPath: projectFile(project.id).path))
        await #expect(throws: LocalhostError.projectNotFound(project.id)) {
            try await library.removeProject(id: project.id, origin: .mainWindow)
        }
    }

    // MARK: Services

    @Test func `adds, edits and removes services`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        let api = try await library.addService(
            toProject: project.id, name: "API", key: Key("api"), workingDirectory: "./api/",
            commandLine: "  uv run fastapi dev\n", origin: .mainWindow
        )
        #expect(api.workingDirectory == "api")
        #expect(api.commandLine == "uv run fastapi dev")

        let edited = try await library.editService(
            id: api.id, commandLine: "uv run fastapi dev --port 8001", variables: VariableInput.plain(["DEBUG": "1"]), origin: .mainWindow
        )
        #expect(edited.name == "API")
        #expect(edited.environment == ["DEBUG": "1"])
        #expect(try await makeLibrary().project(id: project.id).services == [edited])

        try await library.removeService(id: api.id, origin: .mainWindow)
        #expect(try await makeLibrary().project(id: project.id).services.isEmpty)
        await #expect(throws: LocalhostError.serviceNotFound(api.id)) {
            try await library.removeService(id: api.id, origin: .mainWindow)
        }
    }

    @Test func `service keys are unique per project, not app-wide`() async throws {
        let library = try makeLibrary()
        let one = try await library.addProject(name: "One", key: Key("one"), path: "/tmp/one", origin: .mainWindow)
        let two = try await library.addProject(name: "Two", key: Key("two"), path: "/tmp/two", origin: .mainWindow)
        try await library.addService(toProject: one.id, name: "Web", key: Key("web"), commandLine: "pnpm dev", origin: .mainWindow)
        try await library.addService(toProject: two.id, name: "Web", key: Key("web"), commandLine: "pnpm dev", origin: .mainWindow)
        let api = try await library.addService(toProject: one.id, name: "API", key: Key("api"), commandLine: "make api", origin: .mainWindow)

        await #expect(throws: LocalhostError.duplicateServiceKey(try Key("web"), projectID: one.id)) {
            try await library.addService(toProject: one.id, name: "Web 2", key: Key("web"), commandLine: "pnpm dev", origin: .mainWindow)
        }
        await #expect(throws: LocalhostError.duplicateServiceKey(try Key("web"), projectID: one.id)) {
            try await library.editService(id: api.id, key: Key("web"), origin: .mainWindow)
        }
    }

    @Test(arguments: ["", ".", "./", " . "])
    func `an empty working directory means the project root`(raw: String) async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        let service = try await library.addService(
            toProject: project.id, name: "All", key: Key("all"), workingDirectory: raw, commandLine: "make", origin: .mainWindow
        )
        #expect(service.workingDirectory == ".")
        #expect(project.workingDirectoryURL(for: service).path == "/tmp/app")
    }

    @Test func `working directories resolve against the project path`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        let service = try await library.addService(
            toProject: project.id, name: "Shared", key: Key("shared"), workingDirectory: "../shared/tools",
            commandLine: "make", origin: .mainWindow
        )
        #expect(project.workingDirectoryURL(for: service).path == "/tmp/shared/tools")
    }

    @Test(arguments: ["/tmp/app/web", "~/web"])
    func `rejects an absolute working directory`(raw: String) async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        await #expect(throws: LocalhostError.invalidWorkingDirectory(raw)) {
            try await library.addService(
                toProject: project.id, name: "Web", key: Key("web"), workingDirectory: raw, commandLine: "pnpm dev", origin: .mainWindow
            )
        }
    }

    @Test func `rejects an empty command line`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        await #expect(throws: LocalhostError.emptyCommandLine) {
            try await library.addService(toProject: project.id, name: "Web", key: Key("web"), commandLine: " \n", origin: .mainWindow)
        }
    }

    @Test(arguments: ["", "1PORT", "MY-VAR", "MY VAR", "ÉTÉ"])
    func `rejects invalid environment variable names`(name: String) async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        await #expect(throws: LocalhostError.invalidEnvironmentVariableName(name)) {
            try await library.addService(
                toProject: project.id, name: "Web", key: Key("web"), commandLine: "pnpm dev",
                variables: VariableInput.plain([name: "x", "_OK_1": "y"]), origin: .mainWindow
            )
        }
    }

    @Test func `adding a service to a missing project fails`() async throws {
        let id = UUID()
        await #expect(throws: LocalhostError.projectNotFound(id)) {
            try await makeLibrary().addService(toProject: id, name: "Web", key: Key("web"), commandLine: "pnpm dev", origin: .mainWindow)
        }
    }

    // MARK: Persistence failures

    @Test func `a failed write leaves the library unchanged`() async throws {
        let library = try makeLibrary()
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        let projectsDirectory = root.appending(path: "projects")
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: projectsDirectory.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: projectsDirectory.path) }

        await #expect {
            try await library.editProject(id: project.id, name: "Renamed", origin: .mainWindow)
        } throws: { error in
            guard case LocalhostError.persistenceFailed = error else { return false }
            return true
        }
        #expect(try await library.project(id: project.id).name == "App")
    }

    @Test func `refuses to load a file from an unknown schema version`() throws {
        let projectsDirectory = root.appending(path: "projects")
        try FileManager.default.createDirectory(at: projectsDirectory, withIntermediateDirectories: true)
        let file = projectsDirectory.appending(path: "\(UUID().uuidString).json")
        try Data(#"{"schemaVersion": 99, "project": {}}"#.utf8).write(to: file)

        #expect(throws: LocalhostError.unsupportedSchemaVersion(99, path: file.path)) {
            try makeLibrary()
        }
    }

    @Test func `reports a corrupt file by path`() throws {
        let projectsDirectory = root.appending(path: "projects")
        try FileManager.default.createDirectory(at: projectsDirectory, withIntermediateDirectories: true)
        let file = projectsDirectory.appending(path: "\(UUID().uuidString).json")
        try Data("not json".utf8).write(to: file)

        #expect {
            try makeLibrary()
        } throws: { error in
            guard case LocalhostError.persistenceFailed(let path, _) = error else { return false }
            return path == file.path
        }
    }

    // MARK: Events

    @Test func `broadcasts every change with its origin`() async throws {
        let library = try makeLibrary()
        var events = await library.events().makeAsyncIterator()

        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
        let service = try await library.addService(
            toProject: project.id, name: "Web", key: Key("web"), commandLine: "pnpm dev", origin: .menuBar
        )
        try await library.removeProject(id: project.id, origin: .menuBar)

        #expect(await events.next() == .projectAdded(project, origin: .mainWindow))
        guard case .projectUpdated(let updated, .menuBar) = await events.next() else {
            Issue.record("expected projectUpdated from the menu bar")
            return
        }
        #expect(updated.services == [service])
        #expect(await events.next() == .projectRemoved(id: project.id, origin: .menuBar))
    }

    @Test func `subscribe returns the current projects with the stream`() async throws {
        let library = try makeLibrary()
        let existing = try await library.addProject(name: "A", key: Key("a"), path: "/tmp/a", origin: .mainWindow)

        let (projects, events) = await library.subscribe()
        var iterator = events.makeAsyncIterator()
        let added = try await library.addProject(name: "B", key: Key("b"), path: "/tmp/b", origin: .menuBar)

        #expect(projects == [existing])
        #expect(await iterator.next() == .projectAdded(added, origin: .menuBar))
    }

    @Test func `failed changes are not broadcast`() async throws {
        let library = try makeLibrary()
        var events = await library.events().makeAsyncIterator()

        _ = try? await library.addProject(name: "", key: Key("bad"), path: "/tmp/bad", origin: .mainWindow)
        let project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)

        #expect(await events.next() == .projectAdded(project, origin: .mainWindow))
    }
}
