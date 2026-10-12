import AppKit
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
        case detectServices(projectID: UUID)
        case editService(UUID)
        case projectEnvironment(UUID)
        /// The review step of an import, before anything is saved.
        case importProject(ProjectDocument, fileName: String)

        var id: Self { self }
    }

    enum SidebarItem: Hashable {
        case ports
        case project(UUID)
    }

    /// A start that failed because its port is taken, waiting for the user
    /// to stop the holder, start on another port, or cancel.
    struct BlockedStart: Identifiable {
        let serviceID: UUID
        let port: Int
        let holder: Listener?
        let nextFreePort: Int?

        var id: UUID { serviceID }
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
    var sidebarSelection: SidebarItem? {
        didSet {
            guard sidebarSelection != oldValue else { return }
            selectedServiceID = selectedProject?.services.first?.id
            saveSelection()
        }
    }
    var selectedProjectID: UUID? {
        get { if case .project(let id) = sidebarSelection { id } else { nil } }
        set { sidebarSelection = newValue.map(SidebarItem.project) }
    }
    var selectedServiceID: UUID? {
        didSet { if selectedServiceID != oldValue { saveSelection() } }
    }
    /// Text the sidebar's Projects are filtered by.
    var projectSearch = ""
    /// The Projects whose name or Key contains `projectSearch`.
    var filteredProjects: [Project] {
        let query = projectSearch.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return projects }
        return projects.filter {
            $0.name.localizedStandardContains(query) || $0.key.rawValue.localizedStandardContains(query)
        }
    }
    /// Text the log views are filtered by (⌘F focuses it).
    var logFilter = ""
    /// Incremented to ask the visible log view to focus its filter.
    private(set) var logFindCount = 0
    /// Incremented once log buffers were cleared, so log views start over.
    private(set) var logClearCount = 0
    var presentedSheet: Sheet?
    /// Whether the command palette (⌘K) is open over the main window.
    var isShowingPalette = false
    /// The latest request to move keyboard focus to a column of the main window.
    private(set) var columnFocusRequest: ColumnFocusRequest?
    var pendingRemoval: Removal?
    /// A Project file waiting for the save panel to write it.
    var exportingFile: ProjectFile?
    /// Whether the open panel for Import Project… is up.
    var isChoosingImportFile = false
    /// A Project file that couldn't be read, and why.
    var importFailure: ImportFailure?
    /// A failed action to report, e.g. a removal that could not be saved.
    var actionError: LocalhostError?

    /// Orphans found at launch, waiting for the user to stop or leave them.
    private(set) var orphans: [Orphan] = []
    private(set) var orphanDescriptions: [String] = []
    private(set) var isStoppingOrphans = false
    var isShowingOrphanAlert = false

    /// Every Listener, as of the last refresh (only while a Listener view is visible).
    private(set) var listeners: [Listener] = []
    var selectedListenerID: pid_t?
    /// A Listener localhost didn't start, waiting for the user to confirm stopping it.
    var pendingListenerStop: Listener?
    private(set) var stoppingListenerIDs: Set<pid_t> = []
    var blockedStart: BlockedStart?

    /// Unmanaged Listeners holding a configured Port, as of the last refresh
    /// (only while the menu bar window is open).
    private(set) var portWarnings: [PortWarning] = []
    /// Who asked to quit, for the Origin of the stops that follow.
    @ObservationIgnored var quitOrigin: Origin = .mainWindow
    /// Opens (or focuses) the main window; set by the scene that can open it.
    @ObservationIgnored var showMainWindow: (() -> Void)?

    /// Whether a Service may move to a free port when its Port is taken.
    var isAutomaticPortMoveEnabled: Bool {
        didSet {
            UserDefaults.standard.set(isAutomaticPortMoveEnabled, forKey: Self.automaticPortMoveKey)
            guard let services = core?.services else { return }
            Task { [isAutomaticPortMoveEnabled] in await services.setAutomaticPortMove(isAutomaticPortMoveEnabled) }
        }
    }

    private static let automaticPortMoveKey = "automaticPortMove"

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
        let isAutomaticPortMoveEnabled = UserDefaults.standard.object(forKey: Self.automaticPortMoveKey) as? Bool ?? true
        self.isAutomaticPortMoveEnabled = isAutomaticPortMoveEnabled
        do {
            let library = try ProjectLibrary(storageRoot: root)
            let logs = LogStore(root: root)
            let services = ServiceManager(
                library: library, logs: logs, runRegistry: runRegistry,
                isAutomaticPortMoveEnabled: isAutomaticPortMoveEnabled
            )
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

    /// The `selectedServiceID` of the "All services" row, which shows every
    /// Service of the selected Project in one stream.
    static let allServicesID = UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))

    var isAllServicesSelected: Bool {
        selectedProject != nil && selectedServiceID == Self.allServicesID
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

    /// Starts the Service; if its port is taken, asks what to do (`blockedStart`).
    func start(_ serviceID: UUID, onPort port: Int? = nil, origin: Origin = .mainWindow) {
        guard let services = core?.services else { return }
        Task {
            await report(origin: origin) { () throws(LocalhostError) in
                let status = try await services.start(serviceID, onPort: port, origin: origin)
                await presentIfBlocked(status, origin: origin)
            }
        }
    }

    func stop(_ serviceID: UUID, origin: Origin = .mainWindow) {
        guard let services = core?.services else { return }
        Task { await report(origin: origin) { () throws(LocalhostError) in try await services.stop(serviceID, origin: origin) } }
    }

    func restart(_ serviceID: UUID, origin: Origin = .mainWindow) {
        guard let services = core?.services else { return }
        Task {
            await report(origin: origin) { () throws(LocalhostError) in
                let status = try await services.restart(serviceID, origin: origin)
                await presentIfBlocked(status, origin: origin)
            }
        }
    }

    func startAll(inProject projectID: UUID, origin: Origin = .mainWindow) {
        guard let services = core?.services else { return }
        Task { await report(origin: origin) { () throws(LocalhostError) in try await services.startAll(inProject: projectID, origin: origin) } }
    }

    func stopAll(inProject projectID: UUID, origin: Origin = .mainWindow) {
        guard let services = core?.services else { return }
        Task { await report(origin: origin) { () throws(LocalhostError) in try await services.stopAll(inProject: projectID, origin: origin) } }
    }

    /// The URL "Open" uses for a Service, or `nil` if there is none to open.
    func urlToOpen(_ service: Service) -> URL? {
        service.urlToOpen(for: status(of: service).run)
    }

    func openURL(of serviceID: UUID) {
        guard let (_, service) = service(serviceID), let url = urlToOpen(service) else { return }
        NSWorkspace.shared.open(url)
    }

    // MARK: Logs

    func findInLog() {
        logFindCount += 1
    }

    /// Moves keyboard focus to a column of the main window (see `ColumnNavigation`).
    func focusColumn(_ column: MainColumn) {
        columnFocusRequest = ColumnFocusRequest(column: column)
    }

    /// Whether a log is shown that ⌃L can clear.
    var canClearLogs: Bool {
        selectedService != nil || isAllServicesSelected
    }

    /// Clears the shown log's buffers: the selected Service's, or every
    /// Service's of the Project for "All services". Log files are kept.
    func clearLogs() {
        guard let logs = core?.logs else { return }
        let serviceIDs = isAllServicesSelected
            ? selectedProject?.services.map(\.id) ?? []
            : selectedService.map { [$0.id] } ?? []
        Task {
            for serviceID in serviceIDs {
                await logs.clearBuffer(ofService: serviceID)
            }
            logClearCount += 1
        }
    }

    func stopAllServices(origin: Origin = .mainWindow) async {
        await core?.services.stopAll(origin: origin)
    }

    // MARK: Editing

    /// Suggestions for a folder being added as a Project.
    func suggestions(forFolderAt path: String) async -> [ServiceSuggestion] {
        await ProjectScanner().scan(projectAt: path)
    }

    /// Suggestions for an existing Project, without what its Services cover.
    func suggestions(for project: Project) async -> [ServiceSuggestion] {
        await ProjectScanner().scan(project)
    }

    @discardableResult
    func addProject(name: String, key: String, path: String, services: [NewService] = []) async throws(LocalhostError) -> Project {
        guard let library = core?.library else { throw .projectNotFound(UUID()) }
        let project = try await library.addProject(name: name, key: Key(key), path: path, services: services, origin: .mainWindow)
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
        commandLine: String, variables: [VariableInput], dependencies: [UUID], isExcludedFromStartAll: Bool,
        port: Int?, url: String?, portCanChange: Bool
    ) async throws(LocalhostError) -> Service {
        guard let library = core?.library else { throw .projectNotFound(projectID) }
        let service = try await library.addService(
            toProject: projectID, name: name, key: Key(key), workingDirectory: workingDirectory,
            commandLine: commandLine, variables: variables, dependencies: dependencies,
            isExcludedFromStartAll: isExcludedFromStartAll, port: port, url: url, portCanChange: portCanChange,
            origin: .mainWindow
        )
        projects = await library.projects()
        selectedServiceID = service.id
        return service
    }

    func addServices(_ services: [NewService], to projectID: UUID) async throws(LocalhostError) {
        guard let library = core?.library else { throw .projectNotFound(projectID) }
        let added = try await library.addServices(services, toProject: projectID, origin: .mainWindow)
        projects = await library.projects()
        if let first = added.first { selectedServiceID = first.id }
    }

    /// Saves a reviewed import. On Replace, Services the file no longer has
    /// are stopped first, as when removing them.
    func importProject(_ document: ProjectDocument, onKeyClash: ProjectLibrary.KeyClash?) async throws(LocalhostError) {
        guard let core else { throw .projectNotFound(UUID()) }
        if onKeyClash == .replace, let existing = projectClashing(with: document) {
            let keptKeys = Set(document.services.map(\.key))
            for service in existing.services where !keptKeys.contains(service.key) {
                try await core.services.stop(service.id, origin: .mainWindow)
            }
        }
        let project = try await core.library.importProject(document, onKeyClash: onKeyClash, origin: .mainWindow)
        projects = await core.library.projects()
        selectedProjectID = project.id
    }

    func editService(
        _ id: UUID, name: String, key: String, workingDirectory: String,
        commandLine: String, variables: [VariableInput], dependencies: [UUID], isExcludedFromStartAll: Bool,
        port: Int?, url: String?, portCanChange: Bool
    ) async throws(LocalhostError) {
        guard let library = core?.library else { throw .serviceNotFound(id) }
        try await library.editService(
            id: id, name: name, key: Key(key), workingDirectory: workingDirectory,
            commandLine: commandLine, variables: variables, dependencies: dependencies,
            isExcludedFromStartAll: isExcludedFromStartAll, port: .some(port), url: .some(url),
            portCanChange: portCanChange, origin: .mainWindow
        )
    }

    // MARK: Environment

    /// One Environment Profile as edited in the Environment sheet.
    struct ProfileDraft: Identifiable {
        let id = UUID()
        /// `nil` for a Profile added in the sheet.
        var profileID: UUID?
        var name: String
        var variables: [VariableDraft]
    }

    func setActiveProfile(_ profileID: UUID?, inProject projectID: UUID) {
        guard let library = core?.library else { return }
        Task { await report { () throws(LocalhostError) in try await library.setActiveProfile(profileID, inProject: projectID, origin: .mainWindow) } }
    }

    /// Saves the Environment sheet: the Project's variables, its Profiles
    /// (added, edited, removed), then which one is active.
    func saveEnvironment(
        of projectID: UUID, variables: [VariableInput], profiles: [ProfileDraft], activeDraftID: UUID?
    ) async throws(LocalhostError) {
        guard let library = core?.library, let project = project(projectID) else { throw .projectNotFound(projectID) }
        try await library.editProject(id: projectID, variables: variables, origin: .mainWindow)
        let kept = Set(profiles.compactMap(\.profileID))
        for profile in project.profiles where !kept.contains(profile.id) {
            try await library.removeProfile(id: profile.id, origin: .mainWindow)
        }
        var activeProfileID: UUID?
        for draft in profiles {
            let inputs = draft.variables.compactMap(\.input)
            let id: UUID
            if let profileID = draft.profileID {
                id = try await library.editProfile(id: profileID, name: draft.name, variables: inputs, origin: .mainWindow).id
            } else {
                id = try await library.addProfile(toProject: projectID, name: draft.name, variables: inputs, origin: .mainWindow).id
            }
            if draft.id == activeDraftID { activeProfileID = id }
        }
        try await library.setActiveProfile(activeProfileID, inProject: projectID, origin: .mainWindow)
    }

    /// A Secret's stored value, to reveal it; `nil` if it has none.
    func secretValue(named name: String, in layer: VariableLayer) async -> String? {
        guard let library = core?.library else { return nil }
        do {
            return try await library.secretValue(named: name, in: layer)
        } catch {
            actionError = error
            return nil
        }
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

    // MARK: Listeners

    func listener(_ id: pid_t) -> Listener? {
        listeners.first { $0.id == id }
    }

    func refreshListeners() async {
        guard let services = core?.services else { return }
        listeners = await services.listeners()
    }

    /// Rescans Listeners for Unmanaged ones holding a configured Port. A
    /// Service that is active is left out: its Run already says why it moved or failed.
    func refreshPortWarnings() async {
        guard let services = core?.services else { return }
        let unmanaged = await services.listeners().filter(\.isUnmanaged)
        var warnings: [PortWarning] = []
        for project in projects {
            for service in project.services {
                guard let port = service.port, !status(of: service).state.isActive,
                      let holder = unmanaged.first(where: { $0.ports.contains(port) })
                else { continue }
                warnings.append(PortWarning(port: port, serviceID: service.id, holder: holder))
            }
        }
        if warnings != portWarnings { portWarnings = warnings }
    }

    /// Why localhost won't stop the Listener, or `nil` if it can.
    func reasonNotStoppable(_ listener: Listener) -> String? {
        if listener.pid == ProcessInfo.processInfo.processIdentifier { return LocalhostError.listenerIsLocalhost.message }
        if let label = listener.launchdLabel { return LocalhostError.listenerIsLaunchdJob(label: label).message }
        if listener.kind == .docker { return LocalhostError.listenerIsDocker.message }
        return nil
    }

    /// Stops a Run's Listener (its Service) at once; asks before stopping any other.
    func requestStop(_ listener: Listener) {
        if case .run(let serviceID, _) = listener.owner {
            stop(serviceID)
        } else {
            pendingListenerStop = listener
        }
    }

    func stopListener(_ listener: Listener) async {
        guard let services = core?.services else { return }
        stoppingListenerIDs.insert(listener.id)
        await report { () throws(LocalhostError) in try await services.stopListener(listener, origin: .mainWindow) }
        stoppingListenerIDs.remove(listener.id)
        await refreshListeners()
    }

    /// Who a Listener belongs to, e.g. "My SaaS › API", "In My SaaS", "Docker", "Unmanaged".
    func ownerDescription(_ listener: Listener) -> String {
        switch listener.owner {
        case .run(let serviceID, _):
            if let (project, service) = service(serviceID) { return "\(project.name) › \(service.name)" }
            return "A removed service"
        case .project(let projectID):
            return "In \(project(projectID)?.name ?? "a removed project")"
        case .unmanaged:
            switch listener.kind {
            case .docker: return "Docker"
            case .app: return "App"
            case .system: return "macOS"
            case .development: return "Unmanaged"
            }
        }
    }

    /// Stops whatever holds the blocked start's port, then starts the Service.
    func stopHolderAndStart(_ blocked: BlockedStart) async {
        guard let services = core?.services, let holder = blocked.holder else { return }
        await report { () throws(LocalhostError) in
            try await services.stopListener(holder, origin: .mainWindow)
        }
        start(blocked.serviceID)
    }

    private func presentIfBlocked(_ status: ServiceStatus, origin: Origin = .mainWindow) async {
        guard status.state == .failed, case .portConflict(let port, let holder) = status.run?.failure,
              let services = core?.services
        else { return }
        let nextFreePort = try? await services.nextFreePort(for: status.serviceID)
        blockedStart = BlockedStart(serviceID: status.serviceID, port: port, holder: holder, nextFreePort: nextFreePort)
        // The question is asked in the main window.
        if origin == .menuBar { showMainWindow?() }
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
            self?.restoreSelection()
            for await event in events {
                guard let self else { return }
                switch event {
                case .projectAdded, .projectUpdated, .projectRemoved:
                    self.projects = await library.projects()
                }
            }
        }
    }

    // MARK: Selection

    private static let sidebarSelectionKey = "selection.sidebar"
    private static let serviceSelectionKey = "selection.service"
    @ObservationIgnored private var isRestoringSelection = false

    /// The last selection, else the first Project's "All services", so the
    /// window never opens on an empty selection.
    private func restoreSelection() {
        guard sidebarSelection == nil, !projects.isEmpty else { return }
        isRestoringSelection = true
        defer { isRestoringSelection = false }
        let defaults = UserDefaults.standard
        let savedSidebar = defaults.string(forKey: Self.sidebarSelectionKey)
        if savedSidebar == "ports" {
            sidebarSelection = .ports
            return
        }
        let project = savedSidebar.flatMap(UUID.init(uuidString:)).flatMap(project) ?? projects[0]
        sidebarSelection = .project(project.id)
        let savedService = defaults.string(forKey: Self.serviceSelectionKey).flatMap(UUID.init(uuidString:))
        if let savedService, savedService == Self.allServicesID || project.services.contains(where: { $0.id == savedService }) {
            selectedServiceID = savedService
        } else {
            selectedServiceID = project.services.isEmpty ? nil : Self.allServicesID
        }
    }

    private func saveSelection() {
        guard !isRestoringSelection else { return }
        let defaults = UserDefaults.standard
        switch sidebarSelection {
        case .ports: defaults.set("ports", forKey: Self.sidebarSelectionKey)
        case .project(let id): defaults.set(id.uuidString, forKey: Self.sidebarSelectionKey)
        case nil: defaults.removeObject(forKey: Self.sidebarSelectionKey)
        }
        defaults.set(selectedServiceID?.uuidString, forKey: Self.serviceSelectionKey)
    }

    private func report(origin: Origin = .mainWindow, _ action: () async throws(LocalhostError) -> Void) async {
        do {
            try await action()
        } catch {
            actionError = error
            // Errors are reported in the main window.
            if origin == .menuBar { showMainWindow?() }
        }
    }
}
