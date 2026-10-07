import Foundation

/// A change to the Project Configuration, tagged with the Origin that caused it.
public enum ProjectLibraryEvent: Sendable, Equatable {
    case projectAdded(Project, origin: Origin)
    /// The Project itself or one of its Services was added, edited or removed.
    case projectUpdated(Project, origin: Origin)
    case projectRemoved(id: UUID, origin: Origin)
}

/// Owns every Project and its Services: validates changes, persists them, and
/// broadcasts them as `ProjectLibraryEvent`s.
///
/// A change is persisted before it becomes visible; if writing fails, the
/// library is left unchanged and the error is thrown.
public actor ProjectLibrary {
    public static let defaultStorageRoot = URL.applicationSupportDirectory
        .appending(path: "localhost", directoryHint: .isDirectory)

    private let storage: ProjectStorage
    private var projectsByID: [UUID: Project]
    private var subscribers: [UUID: AsyncStream<ProjectLibraryEvent>.Continuation] = [:]

    /// Loads every stored Project from `storageRoot`.
    public init(storageRoot: URL = ProjectLibrary.defaultStorageRoot) throws(LocalhostError) {
        storage = ProjectStorage(root: storageRoot)
        let projects = try storage.loadAll()
        projectsByID = Dictionary(uniqueKeysWithValues: projects.map { ($0.id, $0) })
    }

    // MARK: Reading

    /// All Projects, in sort order.
    public func projects() -> [Project] {
        projectsByID.values.sorted { ($0.sortOrder, $0.name) < ($1.sortOrder, $1.name) }
    }

    public func project(id: UUID) throws(LocalhostError) -> Project {
        guard let project = projectsByID[id] else { throw .projectNotFound(id) }
        return project
    }

    /// A Service together with the Project it belongs to.
    public func service(id: UUID) throws(LocalhostError) -> (project: Project, service: Service) {
        let (project, index) = try locateService(id)
        return (project, project.services[index])
    }

    /// All Projects plus a stream of every change after that snapshot. Taking
    /// both at once means no change falls between them.
    public func subscribe() -> (projects: [Project], events: AsyncStream<ProjectLibraryEvent>) {
        (projects(), events())
    }

    /// A stream of every change made after subscribing.
    public func events() -> AsyncStream<ProjectLibraryEvent> {
        let (stream, continuation) = AsyncStream.makeStream(of: ProjectLibraryEvent.self)
        let id = UUID()
        subscribers[id] = continuation
        continuation.onTermination = { [weak self] _ in
            Task { await self?.unsubscribe(id) }
        }
        return stream
    }

    // MARK: Projects

    @discardableResult
    public func addProject(
        name: String,
        key: Key,
        path: String,
        origin: Origin
    ) throws(LocalhostError) -> Project {
        try ensureProjectKeyIsFree(key)
        let nextSortOrder = (projectsByID.values.map(\.sortOrder).max() ?? -1) + 1
        let project = Project(
            name: try Validate.name(name),
            key: key,
            path: try Validate.projectPath(path),
            sortOrder: nextSortOrder
        )
        try commit(project)
        broadcast(.projectAdded(project, origin: origin))
        return project
    }

    /// Changes the given fields of a Project; `nil` leaves a field as it is.
    @discardableResult
    public func editProject(
        id: UUID,
        name: String? = nil,
        key: Key? = nil,
        path: String? = nil,
        isFavorite: Bool? = nil,
        origin: Origin
    ) throws(LocalhostError) -> Project {
        var project = try self.project(id: id)
        if let name { project.name = try Validate.name(name) }
        if let key, key != project.key {
            try ensureProjectKeyIsFree(key)
            project.key = key
        }
        if let path { project.path = try Validate.projectPath(path) }
        if let isFavorite { project.isFavorite = isFavorite }
        try commit(project)
        broadcast(.projectUpdated(project, origin: origin))
        return project
    }

    public func removeProject(id: UUID, origin: Origin) throws(LocalhostError) {
        guard projectsByID[id] != nil else { throw .projectNotFound(id) }
        try storage.delete(id)
        projectsByID[id] = nil
        broadcast(.projectRemoved(id: id, origin: origin))
    }

    // MARK: Services

    @discardableResult
    public func addService(
        toProject projectID: UUID,
        name: String,
        key: Key,
        workingDirectory: String = ".",
        commandLine: String,
        environment: [String: String] = [:],
        dependencies: [UUID] = [],
        isExcludedFromStartAll: Bool = false,
        origin: Origin
    ) throws(LocalhostError) -> Service {
        var project = try self.project(id: projectID)
        try ensureServiceKeyIsFree(key, in: project)
        let service = Service(
            name: try Validate.name(name),
            key: key,
            workingDirectory: try Validate.workingDirectory(workingDirectory),
            commandLine: try Validate.commandLine(commandLine),
            environment: try Validate.environment(environment),
            isExcludedFromStartAll: isExcludedFromStartAll
        )
        project.services.append(service)
        try setDependencies(dependencies, of: service.id, in: &project)
        try commit(project)
        broadcast(.projectUpdated(project, origin: origin))
        return project.services[project.services.count - 1]
    }

    /// Changes the given fields of a Service; `nil` leaves a field as it is.
    @discardableResult
    public func editService(
        id: UUID,
        name: String? = nil,
        key: Key? = nil,
        workingDirectory: String? = nil,
        commandLine: String? = nil,
        environment: [String: String]? = nil,
        dependencies: [UUID]? = nil,
        isExcludedFromStartAll: Bool? = nil,
        origin: Origin
    ) throws(LocalhostError) -> Service {
        var (project, index) = try locateService(id)
        var service = project.services[index]
        if let name { service.name = try Validate.name(name) }
        if let key, key != service.key {
            try ensureServiceKeyIsFree(key, in: project)
            service.key = key
        }
        if let workingDirectory { service.workingDirectory = try Validate.workingDirectory(workingDirectory) }
        if let commandLine { service.commandLine = try Validate.commandLine(commandLine) }
        if let environment { service.environment = try Validate.environment(environment) }
        if let isExcludedFromStartAll { service.isExcludedFromStartAll = isExcludedFromStartAll }
        project.services[index] = service
        if let dependencies { try setDependencies(dependencies, of: id, in: &project) }
        try commit(project)
        broadcast(.projectUpdated(project, origin: origin))
        return project.services[index]
    }

    /// Removes the Service, and removes it from every other Service's Dependencies.
    public func removeService(id: UUID, origin: Origin) throws(LocalhostError) {
        var (project, index) = try locateService(id)
        project.services.remove(at: index)
        for i in project.services.indices {
            project.services[i].dependencies.removeAll { $0 == id }
        }
        try commit(project)
        broadcast(.projectUpdated(project, origin: origin))
    }

    // MARK: Private

    private func commit(_ project: Project) throws(LocalhostError) {
        try storage.save(project)
        projectsByID[project.id] = project
    }

    /// Replaces the Dependencies of `serviceID`, rejecting Services of other
    /// Projects and cycles. Duplicates are dropped; the order is kept.
    private func setDependencies(_ dependencies: [UUID], of serviceID: UUID, in project: inout Project) throws(LocalhostError) {
        guard let index = project.services.firstIndex(where: { $0.id == serviceID }) else {
            throw .serviceNotFound(serviceID)
        }
        project.services[index].dependencies = []
        var accepted: [UUID] = []
        for dependency in dependencies where !accepted.contains(dependency) {
            guard project.service(id: dependency) != nil else { throw .serviceNotFound(dependency) }
            if let cycle = project.dependencyCycle(from: serviceID, to: dependency) {
                throw .dependencyCycle(cycle)
            }
            accepted.append(dependency)
            project.services[index].dependencies = accepted
        }
    }

    private func locateService(_ id: UUID) throws(LocalhostError) -> (Project, Int) {
        for project in projectsByID.values {
            if let index = project.services.firstIndex(where: { $0.id == id }) {
                return (project, index)
            }
        }
        throw .serviceNotFound(id)
    }

    private func ensureProjectKeyIsFree(_ key: Key) throws(LocalhostError) {
        if projectsByID.values.contains(where: { $0.key == key }) {
            throw .duplicateProjectKey(key)
        }
    }

    private func ensureServiceKeyIsFree(_ key: Key, in project: Project) throws(LocalhostError) {
        if project.services.contains(where: { $0.key == key }) {
            throw .duplicateServiceKey(key, projectID: project.id)
        }
    }

    private func broadcast(_ event: ProjectLibraryEvent) {
        for continuation in subscribers.values {
            continuation.yield(event)
        }
    }

    private func unsubscribe(_ id: UUID) {
        subscribers[id] = nil
    }
}

/// Field validation and normalization shared by Project and Service edits.
private enum Validate {
    static func name(_ raw: String) throws(LocalhostError) -> String {
        let name = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { throw .invalidName }
        return name
    }

    /// An absolute path (a leading `~` is expanded), standardized.
    static func projectPath(_ raw: String) throws(LocalhostError) -> String {
        let expanded = (raw.trimmingCharacters(in: .whitespacesAndNewlines) as NSString).expandingTildeInPath
        guard expanded.hasPrefix("/") else { throw .invalidProjectPath(raw) }
        return URL(filePath: expanded).standardizedFileURL.path
    }

    /// A path relative to the Project, e.g. `web` or `../shared`; empty means `.`.
    static func workingDirectory(_ raw: String) throws(LocalhostError) -> String {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.hasPrefix("/"), !trimmed.hasPrefix("~") else {
            throw .invalidWorkingDirectory(raw)
        }
        let components = trimmed.split(separator: "/").filter { $0 != "." }
        return components.isEmpty ? "." : components.joined(separator: "/")
    }

    static func commandLine(_ raw: String) throws(LocalhostError) -> String {
        let commandLine = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !commandLine.isEmpty else { throw .emptyCommandLine }
        return commandLine
    }

    /// Variable names must be portable shell names: `[A-Za-z_][A-Za-z0-9_]*`.
    static func environment(_ environment: [String: String]) throws(LocalhostError) -> [String: String] {
        for name in environment.keys.sorted() where !isValidVariableName(name) {
            throw .invalidEnvironmentVariableName(name)
        }
        return environment
    }

    private static func isValidVariableName(_ name: String) -> Bool {
        func isLetterOrUnderscore(_ scalar: Unicode.Scalar) -> Bool {
            scalar == "_" || ("a"..."z").contains(scalar) || ("A"..."Z").contains(scalar)
        }
        guard let first = name.unicodeScalars.first, isLetterOrUnderscore(first) else { return false }
        return name.unicodeScalars.allSatisfy { isLetterOrUnderscore($0) || ("0"..."9").contains($0) }
    }
}
