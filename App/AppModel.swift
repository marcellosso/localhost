import Foundation
import LocalhostCore
import Observation

/// The app's single owner of the core. Views read it and call its actions;
/// they never own Runs or processes.
@Observable
final class AppModel {
    struct Core {
        let library: ProjectLibrary
        let services: ServiceManager
        let logs: LogStore
        let statuses: ServiceStatusStore
    }

    enum Sheet: Identifiable, Hashable {
        case addProject
        case editProject(UUID)
        case addService(projectID: UUID)
        case editService(UUID)

        var id: Self { self }
    }

    enum Removal: Identifiable, Hashable {
        case project(UUID)
        case service(UUID)

        var id: Self { self }
    }

    let core: Core?
    /// Why the core could not start, e.g. an unreadable Project file.
    let startupError: LocalhostError?

    private(set) var projects: [Project] = []
    var selectedProjectID: UUID? {
        didSet { if selectedProjectID != oldValue { selectedServiceID = selectedProject?.services.first?.id } }
    }
    var selectedServiceID: UUID?
    var presentedSheet: Sheet?
    var pendingRemoval: Removal?
    /// A failed action to report, e.g. a removal that could not be saved.
    var actionError: LocalhostError?

    /// Orphans found at launch, waiting for the user to stop or leave them.
    private(set) var orphans: [Orphan] = []
    private(set) var orphanDescriptions: [String] = []
    private(set) var isStoppingOrphans = false
    var isShowingOrphanAlert = false

    @ObservationIgnored private let runRegistry: RunRegistry
    @ObservationIgnored private var projectObservation: Task<Void, Never>?

    /// Where Projects, logs and Run records live. `LOCALHOST_STORAGE_ROOT`
    /// overrides it, e.g. to try a build without touching real data.
    static var storageRoot: URL {
        if let override = ProcessInfo.processInfo.environment["LOCALHOST_STORAGE_ROOT"], !override.isEmpty {
            return URL(filePath: override, directoryHint: .isDirectory)
        }
        return ProjectLibrary.defaultStorageRoot
    }

    init() {
        let root = Self.storageRoot
        let runRegistry = RunRegistry(root: root)
        self.runRegistry = runRegistry
        do {
            let library = try ProjectLibrary(storageRoot: root)
            let logs = LogStore(root: root)
            let services = ServiceManager(library: library, logs: logs, runRegistry: runRegistry)
            core = Core(library: library, services: services, logs: logs, statuses: ServiceStatusStore(manager: services))
            startupError = nil
        } catch {
            core = nil
            startupError = error
        }
        observeProjects()
    }

    // MARK: Reading

    var selectedProject: Project? {
        projects.first { $0.id == selectedProjectID }
    }

    var selectedService: Service? {
        selectedProject?.services.first { $0.id == selectedServiceID }
    }

    func project(_ id: UUID) -> Project? {
        projects.first { $0.id == id }
    }

    func service(_ id: UUID) -> (project: Project, service: Service)? {
        for project in projects {
            if let service = project.services.first(where: { $0.id == id }) { return (project, service) }
        }
        return nil
    }

    func status(of service: Service) -> ServiceStatus {
        core?.statuses.status(of: service.id) ?? .neverRun(service.id)
    }

    /// The most significant state among a Project's Services, or `nil` if it has none.
    func aggregateState(of project: Project) -> ServiceState? {
        let states = project.services.map { status(of: $0).state }
        guard !states.isEmpty else { return nil }
        let priority: [ServiceState] = [.failed, .stopping, .starting, .running, .exited, .stopped]
        return priority.first(where: states.contains)
    }

    var activeServiceCount: Int {
        core?.statuses.statuses.values.filter(\.state.isActive).count ?? 0
    }

    /// A Key derived from `name` that no other Project uses.
    func suggestedProjectKey(for name: String, excluding projectID: UUID? = nil) -> String {
        let taken = Set(projects.filter { $0.id != projectID }.map(\.key.rawValue))
        return Self.uniqueKey(from: name, fallback: "project", taken: taken)
    }

    /// A Key derived from `name` that no other Service of the Project uses.
    func suggestedServiceKey(for name: String, in project: Project, excluding serviceID: UUID? = nil) -> String {
        let taken = Set(project.services.filter { $0.id != serviceID }.map(\.key.rawValue))
        return Self.uniqueKey(from: name, fallback: "service", taken: taken)
    }

    private static func uniqueKey(from name: String, fallback: String, taken: Set<String>) -> String {
        let base = Key(suggestedFrom: name)?.rawValue ?? fallback
        var candidate = base
        var suffix = 2
        while taken.contains(candidate) {
            candidate = "\(base.prefix(Key.maxLength - 4))-\(suffix)"
            suffix += 1
        }
        return candidate
    }

    // MARK: Lifecycle

    func start(_ serviceID: UUID) {
        guard let services = core?.services else { return }
        Task { await report { () throws(LocalhostError) in try await services.start(serviceID, origin: .mainWindow) } }
    }

    func stop(_ serviceID: UUID) {
        guard let services = core?.services else { return }
        Task { await report { () throws(LocalhostError) in try await services.stop(serviceID, origin: .mainWindow) } }
    }

    func restart(_ serviceID: UUID) {
        guard let services = core?.services else { return }
        Task { await report { () throws(LocalhostError) in try await services.restart(serviceID, origin: .mainWindow) } }
    }

    func stopAllServices() async {
        await core?.services.stopAll(origin: .mainWindow)
    }

    // MARK: Editing

    @discardableResult
    func addProject(name: String, key: String, path: String) async throws(LocalhostError) -> Project {
        guard let library = core?.library else { throw .projectNotFound(UUID()) }
        let project = try await library.addProject(name: name, key: Key(key), path: path, origin: .mainWindow)
        projects = await library.projects()
        selectedProjectID = project.id
        return project
    }

    func editProject(_ id: UUID, name: String, key: String, path: String) async throws(LocalhostError) {
        guard let library = core?.library else { throw .projectNotFound(id) }
        try await library.editProject(id: id, name: name, key: Key(key), path: path, origin: .mainWindow)
    }

    func setFavorite(_ id: UUID, _ isFavorite: Bool) {
        guard let library = core?.library else { return }
        Task { await report { () throws(LocalhostError) in try await library.editProject(id: id, isFavorite: isFavorite, origin: .mainWindow) } }
    }

    /// Stops the Project's Services, then removes it.
    func removeProject(_ id: UUID) async {
        guard let core, let project = project(id) else { return }
        await report { () throws(LocalhostError) in
            for service in project.services {
                try await core.services.stop(service.id, origin: .mainWindow)
            }
            try await core.library.removeProject(id: id, origin: .mainWindow)
        }
        if selectedProjectID == id { selectedProjectID = projects.first?.id }
    }

    @discardableResult
    func addService(
        to projectID: UUID, name: String, key: String, workingDirectory: String,
        commandLine: String, environment: [String: String]
    ) async throws(LocalhostError) -> Service {
        guard let library = core?.library else { throw .projectNotFound(projectID) }
        let service = try await library.addService(
            toProject: projectID, name: name, key: Key(key), workingDirectory: workingDirectory,
            commandLine: commandLine, environment: environment, origin: .mainWindow
        )
        projects = await library.projects()
        selectedServiceID = service.id
        return service
    }

    func editService(
        _ id: UUID, name: String, key: String, workingDirectory: String,
        commandLine: String, environment: [String: String]
    ) async throws(LocalhostError) {
        guard let library = core?.library else { throw .serviceNotFound(id) }
        try await library.editService(
            id: id, name: name, key: Key(key), workingDirectory: workingDirectory,
            commandLine: commandLine, environment: environment, origin: .mainWindow
        )
    }

    /// Stops the Service, then removes it.
    func removeService(_ id: UUID) async {
        guard let core else { return }
        await report { () throws(LocalhostError) in
            try await core.services.stop(id, origin: .mainWindow)
            try await core.library.removeService(id: id, origin: .mainWindow)
        }
        if selectedServiceID == id { selectedServiceID = nil }
    }

    // MARK: Orphans

    var orphanAlertTitle: String {
        orphans.count == 1
            ? "1 service from the last session is still running"
            : "\(orphans.count) services from the last session are still running"
    }

    /// Looks for Orphans of a previous session. Call once at launch, before
    /// anything is started.
    func checkForOrphans() async {
        let orphans = runRegistry.orphans()
        guard !orphans.isEmpty else { return }
        orphanDescriptions = orphans.map { orphan in
            if let (project, service) = service(orphan.serviceID) {
                "\(project.name) › \(service.name)"
            } else {
                "A removed service (process group \(orphan.processGroupID))"
            }
        }
        self.orphans = orphans
        isShowingOrphanAlert = true
    }

    func stopOrphans() async {
        isStoppingOrphans = true
        await runRegistry.stopOrphans(orphans)
        orphans = []
        isStoppingOrphans = false
    }

    func leaveOrphans() {
        runRegistry.leaveOrphans(orphans)
        orphans = []
    }

    // MARK: Private

    private func observeProjects() {
        guard let library = core?.library else { return }
        projectObservation = Task { [weak self] in
            let (projects, events) = await library.subscribe()
            self?.projects = projects
            for await event in events {
                guard let self else { return }
                switch event {
                case .projectAdded, .projectUpdated, .projectRemoved:
                    self.projects = await library.projects()
                }
            }
        }
    }

    private func report(_ action: () async throws(LocalhostError) -> Void) async {
        do {
            try await action()
        } catch {
            actionError = error
        }
    }
}
