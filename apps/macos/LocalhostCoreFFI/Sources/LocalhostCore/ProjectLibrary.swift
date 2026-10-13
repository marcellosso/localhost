import Foundation

// Every Project, stored as one JSON file each, with Secrets in the Keychain.
// The core validates and saves; these keep the Swift core's labels, Int
// ports and typed errors.

extension ProjectLibrary {
    public typealias KeyClash = LocalhostCore.KeyClash

    /// `~/Library/Application Support/localhost`.
    public static var defaultStorageRoot: URL {
        URL(corePath: LocalhostCore.defaultStorageRoot(), isDirectory: true)
    }

    /// Loads every stored Project. Files that can't be read are listed in
    /// `unreadableFiles`; only an unreadable folder fails.
    public convenience init(storageRoot: URL = ProjectLibrary.defaultStorageRoot) throws(LocalhostError) {
        let library = try core { try ProjectLibrary.ffiOpen(storageRoot: storageRoot.path(percentEncoded: false)) }
        self.init(unsafeFromHandle: library.uniffiCloneHandle())
    }

    /// The Project files that couldn't be loaded, to tell the user once.
    public var unreadableFiles: [UnreadableProjectFile] { ffiUnreadableFiles() }

    // MARK: Reading

    public func projects() -> [Project] {
        ffiProjects()
    }

    public func project(id: UUID) throws(LocalhostError) -> Project {
        try core { try ffiProject(id: id) }
    }

    public func service(id: UUID) throws(LocalhostError) -> (project: Project, service: Service) {
        for project in ffiProjects() {
            if let service = project.service(id: id) { return (project, service) }
        }
        throw .ServiceNotFound(id)
    }

    public func secretValue(named name: String, in layer: VariableLayer) async throws(LocalhostError) -> String? {
        try await core { try await ffiSecretValue(name: name, layer: layer) }
    }

    public func isSecretSet(named name: String, in layer: VariableLayer) async throws(LocalhostError) -> Bool {
        try await core { try await ffiIsSecretSet(name: name, layer: layer) }
    }

    /// Every Project, and a stream of every change after that.
    public func subscribe() -> (projects: [Project], events: AsyncStream<ProjectLibraryEvent>) {
        let subscription = ffiSubscribe()
        return (subscription.projects, subscription.events.stream)
    }

    public func events() -> AsyncStream<ProjectLibraryEvent> {
        ffiEvents().stream
    }

    // MARK: Projects

    @discardableResult
    public func addProject(
        name: String,
        key: Key,
        path: String,
        services: [NewService] = [],
        origin: Origin
    ) async throws(LocalhostError) -> Project {
        try await core { try await ffiAddProject(name: name, key: key, path: path, services: services, origin: origin) }
    }

    @discardableResult
    public func editProject(
        id: UUID,
        name: String? = nil,
        key: Key? = nil,
        path: String? = nil,
        isFavorite: Bool? = nil,
        variables: [VariableInput]? = nil,
        origin: Origin
    ) async throws(LocalhostError) -> Project {
        let changes = ProjectChanges(name: name, key: key, path: path, isFavorite: isFavorite, variables: variables)
        return try await core { try await ffiEditProject(id: id, changes: changes, origin: origin) }
    }

    public func removeProject(id: UUID, origin: Origin) async throws(LocalhostError) {
        try await core { try await ffiRemoveProject(id: id, origin: origin) }
    }

    /// Adds the Project a Project file describes. If another Project has its
    /// Key, nothing changes unless `onKeyClash` says what to do.
    @discardableResult
    public func importProject(
        _ document: ProjectDocument,
        onKeyClash: KeyClash? = nil,
        origin: Origin
    ) async throws(LocalhostError) -> Project {
        try await core { try await ffiImportProject(document: document, onKeyClash: onKeyClash, origin: origin) }
    }

    /// `key` if no Project uses it, else the first free `key-2`, `key-3`, ….
    public func freeProjectKey(basedOn key: Key) -> Key {
        ffiFreeProjectKey(key: key)
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
    ) async throws(LocalhostError) -> Service {
        let service = NewService(
            name: name,
            key: key,
            workingDirectory: workingDirectory,
            commandLine: commandLine,
            port: port,
            url: url,
            portCanChange: portCanChange,
            isExcludedFromStartAll: isExcludedFromStartAll
        )
        return try await core {
            try await ffiAddService(
                projectId: projectID,
                service: service,
                variables: variables,
                dependencies: dependencies,
                origin: origin
            )
        }
    }

    @discardableResult
    public func addServices(
        _ services: [NewService],
        toProject projectID: UUID,
        origin: Origin
    ) async throws(LocalhostError) -> [Service] {
        try await core { try await ffiAddServices(services: services, projectId: projectID, origin: origin) }
    }

    /// Changes what is given; `port: .some(nil)` clears the port.
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
    ) async throws(LocalhostError) -> Service {
        let changes = ServiceChanges(
            name: name,
            key: key,
            workingDirectory: workingDirectory,
            commandLine: commandLine,
            variables: variables,
            dependencies: dependencies,
            isExcludedFromStartAll: isExcludedFromStartAll,
            port: port.map { $0.map(Int64.init) },
            url: url,
            portCanChange: portCanChange
        )
        return try await core { try await ffiEditService(id: id, changes: changes, origin: origin) }
    }

    public func removeService(id: UUID, origin: Origin) async throws(LocalhostError) {
        try await core { try await ffiRemoveService(id: id, origin: origin) }
    }

    // MARK: Environment Profiles

    @discardableResult
    public func addProfile(
        toProject projectID: UUID,
        name: String,
        variables: [VariableInput] = [],
        origin: Origin
    ) async throws(LocalhostError) -> EnvironmentProfile {
        try await core { try await ffiAddProfile(projectId: projectID, name: name, variables: variables, origin: origin) }
    }

    @discardableResult
    public func editProfile(
        id: UUID,
        name: String? = nil,
        variables: [VariableInput]? = nil,
        origin: Origin
    ) async throws(LocalhostError) -> EnvironmentProfile {
        let changes = ProfileChanges(name: name, variables: variables)
        return try await core { try await ffiEditProfile(id: id, changes: changes, origin: origin) }
    }

    public func removeProfile(id: UUID, origin: Origin) async throws(LocalhostError) {
        try await core { try await ffiRemoveProfile(id: id, origin: origin) }
    }

    /// Makes `profileID` the Project's active profile; `nil` for none.
    public func setActiveProfile(
        _ profileID: UUID?,
        inProject projectID: UUID,
        origin: Origin
    ) async throws(LocalhostError) {
        try await core { try await ffiSetActiveProfile(profileId: profileID, projectId: projectID, origin: origin) }
    }
}
