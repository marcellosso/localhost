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
///
/// Secret values go to the `SecretStore`, never into the Project files:
/// clients learn a Secret's name and whether it is set, and read its value
/// only to reveal it.
public actor ProjectLibrary {
    public static let defaultStorageRoot = URL.applicationSupportDirectory
        .appending(path: "localhost", directoryHint: .isDirectory)

    private let storage: ProjectStorage
    private let secrets: any SecretStore
    private var projectsByID: [UUID: Project]
    private var subscribers: [UUID: AsyncStream<ProjectLibraryEvent>.Continuation] = [:]

    /// Loads every stored Project from `storageRoot`.
    public init(
        storageRoot: URL = ProjectLibrary.defaultStorageRoot,
        secrets: any SecretStore = KeychainSecretStore()
    ) throws(LocalhostError) {
        storage = ProjectStorage(root: storageRoot)
        self.secrets = secrets
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

    /// The value of a Secret, to reveal it. `nil` if it has none stored.
    public func secretValue(named name: String, in layer: VariableLayer) throws(LocalhostError) -> String? {
        try ensureSecretExists(name, in: layer)
        return try secrets.value(for: layer.secretAccount(name))
    }

    /// Whether a Secret has a stored value, without reading it.
    public func isSecretSet(named name: String, in layer: VariableLayer) throws(LocalhostError) -> Bool {
        try secretValue(named: name, in: layer) != nil
    }

    /// The variables of a Run of the Service: the Project's, then the
    /// Service's, then the active Environment Profile's, each overriding the
    /// one before, with Secrets resolved.
    func resolvedVariables(forService id: UUID) throws(LocalhostError) -> [String: String] {
        let (project, service) = try self.service(id: id)
        var layers: [(VariableLayer, Variables)] = [(.project(project.id), project.variables), (.service(id), service.variables)]
        if let profile = project.activeProfile { layers.append((.profile(profile.id), profile.variables)) }
        var resolved: [String: String] = [:]
        for (layer, variables) in layers {
            resolved.merge(variables.values) { _, new in new }
            for name in variables.secretNames {
                guard let value = try secrets.value(for: layer.secretAccount(name)) else { throw .secretMissing(name) }
                resolved[name] = value
            }
        }
        return resolved
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

    /// Adds a Project, with `services` (e.g. reviewed Suggestions) in the
    /// same save: either all of it is added or none of it.
    @discardableResult
    public func addProject(
        name: String,
        key: Key,
        path: String,
        services: [NewService] = [],
        origin: Origin
    ) throws(LocalhostError) -> Project {
        try ensureProjectKeyIsFree(key)
        let nextSortOrder = (projectsByID.values.map(\.sortOrder).max() ?? -1) + 1
        var project = Project(
            name: try Validate.name(name),
            key: key,
            path: try Validate.projectPath(path),
            sortOrder: nextSortOrder
        )
        try append(services, to: &project)
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
        variables: [VariableInput]? = nil,
        origin: Origin
    ) throws(LocalhostError) -> Project {
        var project = try self.project(id: id)
        var staleSecrets: [String] = []
        if let name { project.name = try Validate.name(name) }
        if let key, key != project.key {
            try ensureProjectKeyIsFree(key)
            project.key = key
        }
        if let path { project.path = try Validate.projectPath(path) }
        if let isFavorite { project.isFavorite = isFavorite }
        if let variables {
            (project.variables, staleSecrets) = try apply(variables, to: .project(id), replacing: project.variables)
        }
        try commit(project)
        removeSecrets(staleSecrets)
        broadcast(.projectUpdated(project, origin: origin))
        return project
    }

    public func removeProject(id: UUID, origin: Origin) throws(LocalhostError) {
        guard let project = projectsByID[id] else { throw .projectNotFound(id) }
        try storage.delete(id)
        projectsByID[id] = nil
        removeSecrets(Self.secretAccounts(.project(id), project.variables)
            + project.services.flatMap { Self.secretAccounts(.service($0.id), $0.variables) }
            + project.profiles.flatMap { Self.secretAccounts(.profile($0.id), $0.variables) })
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
        variables: [VariableInput] = [],
        dependencies: [UUID] = [],
        isExcludedFromStartAll: Bool = false,
        port: Int? = nil,
        url: String? = nil,
        portCanChange: Bool = true,
        origin: Origin
    ) throws(LocalhostError) -> Service {
        var project = try self.project(id: projectID)
        try ensureServiceKeyIsFree(key, in: project)
        var service = Service(
            name: try Validate.name(name),
            key: key,
            workingDirectory: try Validate.workingDirectory(workingDirectory),
            commandLine: try Validate.commandLine(commandLine),
            isExcludedFromStartAll: isExcludedFromStartAll,
            port: try Validate.port(port),
            url: try Validate.url(url),
            portCanChange: portCanChange
        )
        (service.variables, _) = try apply(variables, to: .service(service.id), replacing: .empty)
        project.services.append(service)
        try setDependencies(dependencies, of: service.id, in: &project)
        try commit(project)
        broadcast(.projectUpdated(project, origin: origin))
        return project.services[project.services.count - 1]
    }

    /// Adds several Services to a Project in one save (e.g. reviewed
    /// Suggestions): either all of them are added or none.
    @discardableResult
    public func addServices(
        _ services: [NewService],
        toProject projectID: UUID,
        origin: Origin
    ) throws(LocalhostError) -> [Service] {
        var project = try self.project(id: projectID)
        let existingCount = project.services.count
        try append(services, to: &project)
        try commit(project)
        broadcast(.projectUpdated(project, origin: origin))
        return Array(project.services[existingCount...])
    }

    /// Changes the given fields of a Service; `nil` leaves a field as it is.
    /// For `port` and `url`, `.some(nil)` removes the value.
    @discardableResult
    public func editService(
        id: UUID,
        name: String? = nil,
        key: Key? = nil,
        workingDirectory: String? = nil,
        commandLine: String? = nil,
        variables: [VariableInput]? = nil,
        dependencies: [UUID]? = nil,
        isExcludedFromStartAll: Bool? = nil,
        port: Int?? = nil,
        url: String?? = nil,
        portCanChange: Bool? = nil,
        origin: Origin
    ) throws(LocalhostError) -> Service {
        var (project, index) = try locateService(id)
        var service = project.services[index]
        var staleSecrets: [String] = []
        if let name { service.name = try Validate.name(name) }
        if let key, key != service.key {
            try ensureServiceKeyIsFree(key, in: project)
            service.key = key
        }
        if let workingDirectory { service.workingDirectory = try Validate.workingDirectory(workingDirectory) }
        if let commandLine { service.commandLine = try Validate.commandLine(commandLine) }
        if let variables {
            (service.variables, staleSecrets) = try apply(variables, to: .service(id), replacing: service.variables)
        }
        if let isExcludedFromStartAll { service.isExcludedFromStartAll = isExcludedFromStartAll }
        if let port { service.port = try Validate.port(port) }
        if let url { service.url = try Validate.url(url) }
        if let portCanChange { service.portCanChange = portCanChange }
        project.services[index] = service
        if let dependencies { try setDependencies(dependencies, of: id, in: &project) }
        try commit(project)
        removeSecrets(staleSecrets)
        broadcast(.projectUpdated(project, origin: origin))
        return project.services[index]
    }

    /// Removes the Service, and removes it from every other Service's Dependencies.
    public func removeService(id: UUID, origin: Origin) throws(LocalhostError) {
        var (project, index) = try locateService(id)
        let removed = project.services.remove(at: index)
        for i in project.services.indices {
            project.services[i].dependencies.removeAll { $0 == id }
        }
        try commit(project)
        removeSecrets(Self.secretAccounts(.service(id), removed.variables))
        broadcast(.projectUpdated(project, origin: origin))
    }

    // MARK: Environment Profiles

    @discardableResult
    public func addProfile(
        toProject projectID: UUID,
        name: String,
        variables: [VariableInput] = [],
        origin: Origin
    ) throws(LocalhostError) -> EnvironmentProfile {
        var project = try self.project(id: projectID)
        var profile = EnvironmentProfile(name: try Validate.name(name))
        try ensureProfileNameIsFree(profile.name, in: project)
        (profile.variables, _) = try apply(variables, to: .profile(profile.id), replacing: .empty)
        project.profiles.append(profile)
        try commit(project)
        broadcast(.projectUpdated(project, origin: origin))
        return profile
    }

    /// Changes the given fields of a Profile; `nil` leaves a field as it is.
    @discardableResult
    public func editProfile(
        id: UUID,
        name: String? = nil,
        variables: [VariableInput]? = nil,
        origin: Origin
    ) throws(LocalhostError) -> EnvironmentProfile {
        var (project, index) = try locateProfile(id)
        var profile = project.profiles[index]
        var staleSecrets: [String] = []
        if let name {
            let name = try Validate.name(name)
            if name != profile.name { try ensureProfileNameIsFree(name, in: project) }
            profile.name = name
        }
        if let variables {
            (profile.variables, staleSecrets) = try apply(variables, to: .profile(id), replacing: profile.variables)
        }
        project.profiles[index] = profile
        try commit(project)
        removeSecrets(staleSecrets)
        broadcast(.projectUpdated(project, origin: origin))
        return profile
    }

    /// Removes the Profile; if it was active, the Project has none active.
    public func removeProfile(id: UUID, origin: Origin) throws(LocalhostError) {
        var (project, index) = try locateProfile(id)
        let removed = project.profiles.remove(at: index)
        if project.activeProfileID == id { project.activeProfileID = nil }
        try commit(project)
        removeSecrets(Self.secretAccounts(.profile(id), removed.variables))
        broadcast(.projectUpdated(project, origin: origin))
    }

    /// Makes `profileID` the Project's active Profile, or none with `nil`.
    public func setActiveProfile(_ profileID: UUID?, inProject projectID: UUID, origin: Origin) throws(LocalhostError) {
        var project = try self.project(id: projectID)
        if let profileID, project.profile(id: profileID) == nil { throw .profileNotFound(profileID) }
        guard project.activeProfileID != profileID else { return }
        project.activeProfileID = profileID
        try commit(project)
        broadcast(.projectUpdated(project, origin: origin))
    }

    // MARK: Private

    /// The layer's new variables from `inputs`, with Secret values written to
    /// the store, plus the accounts of Secrets that no longer exist, to remove
    /// once the change is committed.
    private func apply(
        _ inputs: [VariableInput], to layer: VariableLayer, replacing current: Variables
    ) throws(LocalhostError) -> (Variables, staleSecrets: [String]) {
        var values: [String: String] = [:]
        var secretValues: [String: String] = [:]
        var names: Set<String> = []
        for input in inputs {
            guard Validate.isValidVariableName(input.name) else { throw .invalidEnvironmentVariableName(input.name) }
            guard names.insert(input.name).inserted else { throw .duplicateVariableName(input.name) }
            switch input.value {
            case .plain(let value):
                values[input.name] = value
            case .secret(let value):
                secretValues[input.name] = value
            case .existingSecret(let previousName):
                guard current.secretNames.contains(previousName) else { throw .secretMissing(previousName) }
                // Read every kept value before writing any: a new Secret may
                // take the name a renamed one had.
                if previousName != input.name {
                    guard let value = try secrets.value(for: layer.secretAccount(previousName)) else {
                        throw .secretMissing(previousName)
                    }
                    secretValues[input.name] = value
                }
            }
        }
        for (name, value) in secretValues {
            try secrets.setValue(value, for: layer.secretAccount(name))
        }
        let secretNames = inputs.compactMap { input -> String? in
            if case .plain = input.value { nil } else { input.name }
        }
        let stale = current.secretNames.filter { !secretNames.contains($0) }.map { layer.secretAccount($0) }
        return (Variables(values: values, secretNames: secretNames.sorted()), stale)
    }

    private static func secretAccounts(_ layer: VariableLayer, _ variables: Variables) -> [String] {
        variables.secretNames.map { layer.secretAccount($0) }
    }

    /// Best effort: a leftover item is harmless, the configuration no longer names it.
    private func removeSecrets(_ accounts: [String]) {
        for account in accounts { try? secrets.removeValue(for: account) }
    }

    private func ensureSecretExists(_ name: String, in layer: VariableLayer) throws(LocalhostError) {
        let variables: Variables
        switch layer {
        case .project(let id): variables = try project(id: id).variables
        case .service(let id): variables = try service(id: id).service.variables
        case .profile(let id):
            let (project, index) = try locateProfile(id)
            variables = project.profiles[index].variables
        }
        guard variables.secretNames.contains(name) else { throw .secretMissing(name) }
    }

    private func locateProfile(_ id: UUID) throws(LocalhostError) -> (Project, Int) {
        for project in projectsByID.values {
            if let index = project.profiles.firstIndex(where: { $0.id == id }) { return (project, index) }
        }
        throw .profileNotFound(id)
    }

    private func ensureProfileNameIsFree(_ name: String, in project: Project) throws(LocalhostError) {
        if project.profiles.contains(where: { $0.name == name }) { throw .duplicateProfileName(name) }
    }

    private func append(_ services: [NewService], to project: inout Project) throws(LocalhostError) {
        for new in services {
            try ensureServiceKeyIsFree(new.key, in: project)
            project.services.append(Service(
                name: try Validate.name(new.name),
                key: new.key,
                workingDirectory: try Validate.workingDirectory(new.workingDirectory),
                commandLine: try Validate.commandLine(new.commandLine),
                isExcludedFromStartAll: new.isExcludedFromStartAll,
                port: try Validate.port(new.port),
                url: try Validate.url(new.url),
                portCanChange: new.portCanChange
            ))
        }
    }

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

    static func port(_ port: Int?) throws(LocalhostError) -> Int? {
        guard let port else { return nil }
        guard (1...65_535).contains(port) else { throw .invalidPort(port) }
        return port
    }

    /// An absolute http(s) URL with a host; blank means none.
    static func url(_ raw: String?) throws(LocalhostError) -> String? {
        guard let trimmed = raw?.trimmingCharacters(in: .whitespacesAndNewlines), !trimmed.isEmpty else { return nil }
        guard let components = URLComponents(string: trimmed),
              ["http", "https"].contains(components.scheme?.lowercased()),
              components.host?.isEmpty == false
        else { throw .invalidURL(trimmed) }
        return trimmed
    }

    /// Variable names must be portable shell names: `[A-Za-z_][A-Za-z0-9_]*`.
    static func isValidVariableName(_ name: String) -> Bool {
        func isLetterOrUnderscore(_ scalar: Unicode.Scalar) -> Bool {
            scalar == "_" || ("a"..."z").contains(scalar) || ("A"..."Z").contains(scalar)
        }
        guard let first = name.unicodeScalars.first, isLetterOrUnderscore(first) else { return false }
        return name.unicodeScalars.allSatisfy { isLetterOrUnderscore($0) || ("0"..."9").contains($0) }
    }
}
