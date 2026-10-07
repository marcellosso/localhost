import Foundation
import Testing
@testable import LocalhostCore

/// The Dependency graph and how the library validates and maintains it.
final class DependencyTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let library: ProjectLibrary
    let project: Project

    init() async throws {
        library = try ProjectLibrary(storageRoot: root)
        project = try await library.addProject(name: "App", key: Key("app"), path: "/tmp/app", origin: .mainWindow)
    }

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    func key(_ raw: String) -> Key {
        try! Key(raw)
    }

    func service(_ key: String, dependsOn dependencies: [Service] = []) async throws -> Service {
        try await library.addService(
            toProject: project.id, name: key, key: Key(key), commandLine: "true",
            dependencies: dependencies.map(\.id), origin: .mainWindow
        )
    }

    func current() async throws -> Project {
        try await library.project(id: project.id)
    }

    // MARK: Editing

    @Test func `stores Dependencies in order, without duplicates, and reloads them`() async throws {
        let database = try await service("database")
        let cache = try await service("cache")
        let api = try await library.addService(
            toProject: project.id, name: "API", key: Key("api"), commandLine: "true",
            dependencies: [database.id, cache.id, database.id], isExcludedFromStartAll: true, origin: .mainWindow
        )

        #expect(api.dependencies == [database.id, cache.id])
        #expect(api.isExcludedFromStartAll)
        let reloaded = try ProjectLibrary(storageRoot: root)
        #expect(try await reloaded.service(id: api.id).service == api)
    }

    @Test func `reads Services saved before Dependencies existed`() async throws {
        let json = #"{"id":"6F9A2C1E-0000-4000-8000-000000000001","name":"Web","key":"web","workingDirectory":".","commandLine":"pnpm dev","environment":{}}"#
        let service = try JSONDecoder().decode(Service.self, from: Data(json.utf8))
        #expect(service.dependencies.isEmpty)
        #expect(!service.isExcludedFromStartAll)
    }

    @Test func `rejects a Dependency on itself`() async throws {
        let api = try await service("api")
        await #expect(throws: LocalhostError.dependencyCycle([key("api"), key("api")])) {
            try await self.library.editService(id: api.id, dependencies: [api.id], origin: .mainWindow)
        }
    }

    @Test func `rejects a Dependency that closes a cycle, naming it`() async throws {
        let database = try await service("database")
        let worker = try await service("worker", dependsOn: [database])
        let api = try await service("api", dependsOn: [worker])

        await #expect(throws: LocalhostError.dependencyCycle([key("database"), key("api"), key("worker"), key("database")])) {
            try await self.library.editService(id: database.id, dependencies: [api.id], origin: .mainWindow)
        }
        #expect(try await current().service(id: database.id)?.dependencies == [])
    }

    @Test func `rejects a Dependency on a Service of another Project`() async throws {
        let api = try await service("api")
        let other = try await library.addProject(name: "Other", key: Key("other"), path: "/tmp/other", origin: .mainWindow)
        let foreign = try await library.addService(
            toProject: other.id, name: "DB", key: Key("db"), commandLine: "true", origin: .mainWindow
        )
        await #expect(throws: LocalhostError.serviceNotFound(foreign.id)) {
            try await self.library.editService(id: api.id, dependencies: [foreign.id], origin: .mainWindow)
        }
    }

    @Test func `removing a Service removes it from every Dependency list`() async throws {
        let database = try await service("database")
        let cache = try await service("cache")
        let api = try await service("api", dependsOn: [database, cache])

        try await library.removeService(id: database.id, origin: .mainWindow)

        #expect(try await current().service(id: api.id)?.dependencies == [cache.id])
    }

    // MARK: Graph

    @Test func `allows a Dependency that closes no cycle`() async throws {
        let database = try await service("database")
        let api = try await service("api", dependsOn: [database])
        let web = try await service("web", dependsOn: [api])
        let project = try await current()

        #expect(project.dependencyCycle(from: web.id, to: database.id) == nil)
        #expect(project.dependencyCycle(from: database.id, to: web.id) == [key("database"), key("web"), key("api"), key("database")])
    }

    @Test func `orders Services after their Dependencies, otherwise in Project order`() async throws {
        let web = try await service("web")
        let api = try await service("api")
        let database = try await service("database")
        let docs = try await service("docs")
        try await library.editService(id: web.id, dependencies: [api.id], origin: .mainWindow)
        try await library.editService(id: api.id, dependencies: [database.id], origin: .mainWindow)
        let project = try await current()

        #expect(project.dependencyOrder(of: [web.id, api.id, database.id, docs.id]) == [database.id, api.id, web.id, docs.id])
        #expect(project.dependencyOrder(of: [web.id, docs.id]) == [web.id, docs.id])
        #expect(project.dependents(of: database.id).map(\.id) == [api.id])
    }
}
