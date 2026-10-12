import Foundation
import Synchronization
import Testing
@testable import LocalhostCore

/// Secrets kept in memory, so tests never touch the login Keychain.
final class InMemorySecretStore: SecretStore {
    let items = Mutex<[String: String]>([:])

    func value(for account: String) throws(LocalhostError) -> String? {
        items.withLock { $0[account] }
    }

    func setValue(_ value: String, for account: String) throws(LocalhostError) {
        items.withLock { $0[account] = value }
    }

    func removeValue(for account: String) throws(LocalhostError) {
        _ = items.withLock { $0.removeValue(forKey: account) }
    }

    var accounts: Set<String> { items.withLock { Set($0.keys) } }
}

/// Project variables, Environment Profiles, Secrets, and "Restart to apply".
@Suite(.timeLimit(.minutes(1)))
final class EnvironmentTests: Sendable {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let secrets = InMemorySecretStore()
    let library: ProjectLibrary
    let manager: ServiceManager
    let project: Project

    init() async throws {
        library = try ProjectLibrary(storageRoot: root, secrets: secrets)
        manager = ServiceManager(
            library: library,
            logs: LogStore(root: root),
            runRegistry: RunRegistry(root: root),
            baseEnvironment: ["PATH": "/usr/bin:/bin", "HOME": NSHomeDirectory(), "SHELL": "/bin/sh"],
            gracePeriod: .milliseconds(200),
            stopPolicy: StopPolicy(interruptTimeout: .milliseconds(500), terminateTimeout: .milliseconds(500))
        )
        let projectFolder = root.appending(path: "project")
        try FileManager.default.createDirectory(at: projectFolder, withIntermediateDirectories: true)
        project = try await library.addProject(name: "App", key: Key("app"), path: projectFolder.path, origin: .mainWindow)
    }

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    var projectFile: URL { root.appending(path: "projects/\(project.id.uuidString).json") }

    /// A Service that writes the given variables to `env.txt`, then keeps running.
    func printingService(_ names: [String], variables: [VariableInput] = [], port: Int? = nil) async throws -> Service {
        let echoes = names.map { "\($0)=$\($0)" }.joined(separator: " ")
        return try await library.addService(
            toProject: project.id, name: "Web", key: Key("web"),
            commandLine: "echo \"\(echoes)\" > env.txt; exec sleep 1000", variables: variables, port: port,
            origin: .mainWindow
        )
    }

    /// Starts the Service and returns what it wrote to `env.txt`.
    func runOutput(_ service: Service) async throws -> String {
        let file = URL(filePath: project.path).appending(path: "env.txt")
        try? FileManager.default.removeItem(at: file)
        try await manager.start(service.id, origin: .mainWindow)
        while !FileManager.default.fileExists(atPath: file.path) { try await Task.sleep(for: .milliseconds(20)) }
        try await Task.sleep(for: .milliseconds(50))
        return try String(contentsOf: file, encoding: .utf8).trimmingCharacters(in: .newlines)
    }

    func nextStatus(of serviceID: UUID, _ events: AsyncStream<ServiceEvent>, where matches: (ServiceStatus) -> Bool) async -> ServiceStatus? {
        for await case .serviceStateChanged(let status) in events where status.serviceID == serviceID && matches(status) {
            return status
        }
        return nil
    }

    // MARK: Layering

    @Test func `a Run gets Project, then Service, then active Profile variables, and PORT over all`() async throws {
        try await library.editProject(
            id: project.id,
            variables: [.plain("A", "project"), .plain("B", "project"), .plain("C", "project"), .plain("PORT", "1")],
            origin: .mainWindow
        )
        let profile = try await library.addProfile(toProject: project.id, name: "Testing", variables: [.plain("C", "profile")], origin: .mainWindow)
        try await library.setActiveProfile(profile.id, inProject: project.id, origin: .mainWindow)
        let service = try await printingService(["A", "B", "C", "PORT"], variables: [.plain("B", "service"), .plain("C", "service")], port: 4567)

        #expect(try await runOutput(service) == "A=project B=service C=profile PORT=4567")
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `an inactive Profile adds nothing`() async throws {
        _ = try await library.addProfile(toProject: project.id, name: "Testing", variables: [.plain("C", "profile")], origin: .mainWindow)
        let service = try await printingService(["C"])

        #expect(try await runOutput(service) == "C=")
        try await manager.stop(service.id, origin: .mainWindow)
    }

    // MARK: Profiles

    @Test func `Profiles have unique names, and removing the active one leaves none active`() async throws {
        let development = try await library.addProfile(toProject: project.id, name: "Development", origin: .mainWindow)
        let testing = try await library.addProfile(toProject: project.id, name: "Testing", origin: .mainWindow)
        await #expect(throws: LocalhostError.duplicateProfileName("Development")) {
            try await self.library.editProfile(id: testing.id, name: "Development", origin: .mainWindow)
        }
        await #expect(throws: LocalhostError.profileNotFound(project.id)) {
            try await self.library.setActiveProfile(self.project.id, inProject: self.project.id, origin: .mainWindow)
        }

        try await library.setActiveProfile(development.id, inProject: project.id, origin: .mainWindow)
        #expect(try await library.project(id: project.id).activeProfile?.name == "Development")
        try await library.removeProfile(id: development.id, origin: .mainWindow)

        let reloaded = try await ProjectLibrary(storageRoot: root, secrets: secrets).project(id: project.id)
        #expect(reloaded.activeProfileID == nil)
        #expect(reloaded.profiles.map(\.name) == ["Testing"])
    }

    @Test func `a Project saved before Profiles existed loads with none`() throws {
        let json = #"{"schemaVersion":1,"project":{"id":"6F9A2C1E-0000-4000-8000-000000000009","name":"Old","key":"old","path":"/tmp/old","sortOrder":0,"isFavorite":false,"services":[{"id":"6F9A2C1E-0000-4000-8000-00000000000A","name":"Web","key":"web","workingDirectory":".","commandLine":"pnpm dev","environment":{"A":"1"}}]}}"#
        let folder = root.appending(path: "old/projects")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try Data(json.utf8).write(to: folder.appending(path: "6F9A2C1E-0000-4000-8000-000000000009.json"))

        let project = try #require(try ProjectStorage(root: root.appending(path: "old")).loadAll().projects.first)
        #expect(project.variables == .empty)
        #expect(project.profiles.isEmpty)
        #expect(project.activeProfileID == nil)
        #expect(project.services.first?.variables == Variables(values: ["A": "1"], secretNames: []))
    }

    // MARK: Secrets

    @Test func `a Secret's value is stored apart from the Project file, and reaches the Run`() async throws {
        let service = try await printingService(["TOKEN", "DEBUG"], variables: [.secret("TOKEN", "s3cr3t-value"), .plain("DEBUG", "1")])

        let json = try String(contentsOf: projectFile, encoding: .utf8)
        #expect(json.contains("TOKEN"))
        #expect(!json.contains("s3cr3t-value"))
        #expect(service.secretNames == ["TOKEN"])
        #expect(service.environment == ["DEBUG": "1"])
        #expect(try await library.isSecretSet(named: "TOKEN", in: .service(service.id)))
        #expect(try await library.secretValue(named: "TOKEN", in: .service(service.id)) == "s3cr3t-value")
        await #expect(throws: LocalhostError.secretMissing("DEBUG")) {
            try await self.library.secretValue(named: "DEBUG", in: .service(service.id))
        }

        #expect(try await runOutput(service) == "TOKEN=s3cr3t-value DEBUG=1")
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `a Secret keeps its value when kept or renamed, and loses it when removed or made plain`() async throws {
        let service = try await printingService([], variables: [.secret("TOKEN", "one"), .secret("OTHER", "two")])
        let layer = VariableLayer.service(service.id)

        try await library.editService(id: service.id, variables: [.existingSecret("API_TOKEN", previousName: "TOKEN"), .existingSecret("OTHER")], origin: .mainWindow)
        #expect(try await library.secretValue(named: "API_TOKEN", in: layer) == "one")
        #expect(try await library.secretValue(named: "OTHER", in: layer) == "two")
        #expect(secrets.accounts == [layer.secretAccount("API_TOKEN"), layer.secretAccount("OTHER")])

        try await library.editService(id: service.id, variables: [.plain("API_TOKEN", "visible")], origin: .mainWindow)
        #expect(secrets.accounts.isEmpty)
        #expect(try await library.service(id: service.id).service.environment == ["API_TOKEN": "visible"])
    }

    @Test func `removing a Service, Profile or Project removes its Secrets`() async throws {
        let service = try await printingService([], variables: [.secret("S", "1")])
        let profile = try await library.addProfile(toProject: project.id, name: "P", variables: [.secret("S", "2")], origin: .mainWindow)
        try await library.editProject(id: project.id, variables: [.secret("S", "3")], origin: .mainWindow)
        #expect(secrets.accounts.count == 3)

        try await library.removeService(id: service.id, origin: .mainWindow)
        try await library.removeProfile(id: profile.id, origin: .mainWindow)
        #expect(secrets.accounts == [VariableLayer.project(project.id).secretAccount("S")])
        try await library.removeProject(id: project.id, origin: .mainWindow)
        #expect(secrets.accounts.isEmpty)
    }

    @Test func `a Secret without a stored value fails the start with secretMissing`() async throws {
        let service = try await printingService([], variables: [.secret("TOKEN", "x")])
        try secrets.removeValue(for: VariableLayer.service(service.id).secretAccount("TOKEN"))

        let status = try await manager.start(service.id, origin: .mainWindow)

        #expect(status.state == .failed)
        #expect(status.run?.failure == .secretMissing("TOKEN"))
        #expect(try await library.isSecretSet(named: "TOKEN", in: .service(service.id)) == false)
    }

    @Test func `variable names are validated on every layer`() async throws {
        await #expect(throws: LocalhostError.invalidEnvironmentVariableName("MY-VAR")) {
            try await self.library.editProject(id: self.project.id, variables: [.secret("MY-VAR", "x")], origin: .mainWindow)
        }
        await #expect(throws: LocalhostError.duplicateVariableName("A")) {
            try await self.library.addProfile(toProject: self.project.id, name: "P", variables: [.plain("A", "1"), .secret("A", "2")], origin: .mainWindow)
        }
        #expect(secrets.accounts.isEmpty)
    }

    // MARK: Restart to apply

    @Test func `switching Profiles marks the running Service, and a restart applies it`() async throws {
        let profile = try await library.addProfile(toProject: project.id, name: "Testing", variables: [.plain("MODE", "test")], origin: .mainWindow)
        let service = try await printingService(["MODE"])
        let (_, events) = await manager.subscribe()
        #expect(try await runOutput(service) == "MODE=")

        try await library.setActiveProfile(profile.id, inProject: project.id, origin: .mainWindow)
        #expect(await nextStatus(of: service.id, events) { $0.isRestartNeeded } != nil)

        try await manager.restart(service.id, origin: .mainWindow)
        #expect(await manager.status(of: service.id).isRestartNeeded == false)
        try await manager.stop(service.id, origin: .mainWindow)
        #expect(try await runOutput(service) == "MODE=test")
        try await manager.stop(service.id, origin: .mainWindow)
    }

    @Test func `changing a Secret's value marks it, changing it back clears it, renaming the Service doesn't`() async throws {
        let service = try await printingService(["TOKEN"], variables: [.secret("TOKEN", "a")])
        let (_, events) = await manager.subscribe()
        _ = try await runOutput(service)

        try await library.editService(id: service.id, name: "Renamed", origin: .mainWindow)
        try await library.editService(id: service.id, variables: [.secret("TOKEN", "b")], origin: .mainWindow)
        let marked = await nextStatus(of: service.id, events) { $0.isRestartNeeded }
        #expect(marked?.state.isActive == true)

        try await library.editService(id: service.id, variables: [.secret("TOKEN", "a")], origin: .mainWindow)
        #expect(await nextStatus(of: service.id, events) { !$0.isRestartNeeded } != nil)

        try await manager.stop(service.id, origin: .mainWindow)
        #expect(await manager.status(of: service.id).isRestartNeeded == false)
    }

    @Test func `editing the command line marks a running Service`() async throws {
        let service = try await printingService([])
        let (_, events) = await manager.subscribe()
        _ = try await runOutput(service)

        try await library.editService(id: service.id, commandLine: "exec sleep 999", origin: .mainWindow)

        #expect(await nextStatus(of: service.id, events) { $0.isRestartNeeded } != nil)
        try await manager.stop(service.id, origin: .mainWindow)
    }
}
