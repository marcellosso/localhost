import AppKit
import LocalhostCore
import Observation
import SwiftUI

/// The app's single owner of the core. Views read it and call its actions;
/// they never own Runs or processes.
@Observable
final class AppModel {
    struct Core {
        let library: ProjectLibrary
        /// This machine's shared core, which `localhost mcp` drives too.
        let services: Targets
        let statuses: ServiceStatusStore

        /// The Runs' output, kept where they run.
        var logs: Targets { services }
    }

    /// The forms say where they make their changes: this Mac, or another
    /// Machine, which makes them there.
    enum Sheet: Identifiable, Hashable {
        case addProject(on: Place = .thisMac)
        case editProject(UUID, on: Place = .thisMac)
        case addService(projectID: UUID, on: Place = .thisMac)
        case detectServices(projectID: UUID, on: Place = .thisMac)
        case editService(UUID, on: Place = .thisMac)
        case projectEnvironment(UUID, on: Place = .thisMac)
        /// The review step of an import, before anything is saved. On
        /// another Machine, `folderExists` says whether the file's folder is
        /// there; this Mac checks its own.
        case importProject(ProjectDocument, fileName: String, on: Place = .thisMac, folderExists: Bool = true)
        /// Import Project… on another Machine: choosing the Project file there.
        case chooseProjectFile(machine: UUID)
        /// Every shortcut, grouped (⌘/).
        case keyboardShortcuts
        /// Connect Your Agents, as in Settings › Agents.
        case connectAgents
        /// The first launch's welcome, ending in Add Project….
        case welcome

        var id: Self { self }
    }

    enum SidebarItem: Hashable {
        case ports
        case project(UUID)
        /// A Project of another Machine: identified by both, since each
        /// Machine names its own Projects.
        case machineProject(UUID, machine: UUID)
    }

    /// Where a Project and its Services are: this Mac, or another Machine.
    enum Place: Hashable {
        case thisMac
        case machine(UUID)
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
        case project(UUID, on: Place = .thisMac)
        case service(UUID, on: Place = .thisMac)

        var id: Self { self }
    }

    let core: Core?
    /// Why the core could not start, e.g. an unreadable Project file.
    let startupError: LocalhostError?
    /// The other Machines, kept connected while the app runs.
    let machines: Machines?
    let machineStore: MachinesStore?
    /// Why the list of Machines couldn't be read.
    let machinesError: LocalhostError?

    private(set) var projects: [Project] = []
    var sidebarSelection: SidebarItem? {
        didSet {
            guard sidebarSelection != oldValue else { return }
            selectedServiceID = selectedProject?.services.first?.id
            saveSelection()
        }
    }
    /// The selected Project's id, on this Mac or another Machine. Set, it
    /// selects a Project of this Mac.
    var selectedProjectID: UUID? {
        get {
            switch sidebarSelection {
            case .project(let id), .machineProject(let id, _): id
            case .ports, nil: nil
            }
        }
        set { sidebarSelection = newValue.map(SidebarItem.project) }
    }
    /// Where the selected Project is.
    var selectedPlace: Place {
        if case .machineProject(_, let machineID) = sidebarSelection { .machine(machineID) } else { .thisMac }
    }
    var selectedServiceID: UUID? {
        didSet { if selectedServiceID != oldValue { saveSelection() } }
    }
    /// Text the sidebar's Projects are filtered by.
    var projectSearch = ""
    /// The Projects whose name or Key contains `projectSearch`.
    var filteredProjects: [Project] {
        filtered(projects)
    }

    /// `projects` whose name or Key contains `projectSearch`.
    func filtered(_ projects: [Project]) -> [Project] {
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
    /// The Project whose stats popover (Stats… in its `⋯` menu) is open.
    var statsProjectID: UUID?
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
    /// Orphans the shared core reported, until the launch checks reach them.
    @ObservationIgnored private var foundOrphans: [Orphan] = []
    @ObservationIgnored private var isAskingAboutOrphans = false
    private(set) var orphanDescriptions: [String] = []
    private(set) var isStoppingOrphans = false
    var isShowingOrphanAlert = false

    /// Project files that couldn't be loaded at launch, told once per session.
    var isShowingUnreadableFiles = false
    @ObservationIgnored private var hasReportedUnreadableFiles = false

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
    /// New versions: the sidebar's notice, Check for Updates… and its setting.
    let updates = UpdateController()
    /// Coding agents connected to `localhost mcp`, and `localhost` in terminals.
    let agents = AgentsModel(storageRoot: AppModel.storageRoot)
    /// Anonymous usage data: what is sent is decided, and the choice kept, by the core.
    @ObservationIgnored let telemetry = Telemetry(storageRoot: AppModel.storageRoot)
    /// Whether the sidebar says, once, what is sent and how to turn it off.
    private(set) var isTelemetryNoticePending = false
    /// The first run's welcome and checklist; `nil` when the core couldn't open.
    private(set) var onboarding: Onboarding?
    /// The checklist was asked for again from Help: it stays until closed,
    /// even with every row ticked. Not saved, so the next launch is as before.
    private(set) var isChecklistReplayed = false

    /// What the app sends when it opens, worded once for the notice and Settings.
    static let telemetrySummary = "When localhost opens it sends a random id, its version, your OS and architecture, and how it was installed. Never your projects, commands, paths or logs."

    /// Settings › General's "Share anonymous usage data".
    var isTelemetryEnabled: Bool {
        get {
            access(keyPath: \.isTelemetryEnabled)
            return telemetry.isEnabled
        }
        set {
            var failure: LocalhostError?
            withMutation(keyPath: \.isTelemetryEnabled) {
                do throws(LocalhostError) {
                    try telemetry.setEnabled(newValue)
                } catch {
                    failure = error
                }
            }
            actionError = failure
            isTelemetryNoticePending = false
        }
    }

    /// Whether a Service may move to a free port when its Port is taken.
    var isAutomaticPortMoveEnabled: Bool {
        didSet {
            UserDefaults.standard.set(isAutomaticPortMoveEnabled, forKey: Self.automaticPortMoveKey)
            guard let services = core?.services else { return }
            services.setAutomaticPortMove(isAutomaticPortMoveEnabled)
        }
    }

    private static let automaticPortMoveKey = "automaticPortMove"

    @ObservationIgnored private var coreConnection: Task<Void, Never>?
    @ObservationIgnored private var projectObservation: Task<Void, Never>?
    @ObservationIgnored private var stateAnnouncements: Task<Void, Never>?

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
        let isAutomaticPortMoveEnabled = UserDefaults.standard.object(forKey: Self.automaticPortMoveKey) as? Bool ?? true
        self.isAutomaticPortMoveEnabled = isAutomaticPortMoveEnabled
        do {
            let library = try ProjectLibrary(storageRoot: root)
            let services = Targets(
                library: library, storageRoot: root, isAutomaticPortMoveEnabled: isAutomaticPortMoveEnabled
            )
            core = Core(library: library, services: services, statuses: ServiceStatusStore(manager: services))
            startupError = nil
        } catch {
            core = nil
            startupError = error
        }
        do {
            let machines = try Machines(storageRoot: root)
            self.machines = machines
            machineStore = MachinesStore(machines: machines)
            machinesError = nil
        } catch {
            machines = nil
            machineStore = nil
            machinesError = error
        }
        observeProjects()
        connectToMachines()
        announceStates()
        announceMachineStates()
        connectToSharedCore()
        updates.model = self
        isTelemetryNoticePending = telemetry.isNoticePending
        startOnboarding()
        // Returns at once: the event goes out on a thread of its own, or not at all.
        telemetry.recordAppOpen()
    }

    // MARK: Reading

    /// The selected Project, on this Mac or another Machine.
    var selectedProject: Project? {
        guard let id = selectedProjectID else { return nil }
        return project(id, on: selectedPlace)
    }

    /// The selected Project when it is on this Mac: its folder is here, and it can be exported.
    var selectedLocalProject: Project? {
        selectedPlace == .thisMac ? selectedProject : nil
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

    func project(_ id: UUID, on place: Place = .thisMac) -> Project? {
        switch place {
        case .thisMac: projects.first { $0.id == id }
        case .machine(let machineID): machineStore?.machine(machineID)?.project(id)
        }
    }

    func service(_ id: UUID, on place: Place = .thisMac) -> (project: Project, service: Service)? {
        switch place {
        case .thisMac:
            for project in projects {
                if let service = project.services.first(where: { $0.id == id }) { return (project, service) }
            }
            return nil
        case .machine(let machineID):
            return machineStore?.machine(machineID)?.service(id)
        }
    }

    func status(of service: Service, on place: Place = .thisMac) -> ServiceStatus {
        switch place {
        case .thisMac: core?.statuses.status(of: service.id) ?? .neverRun(service.id)
        case .machine(let machineID): machineStore?.machine(machineID)?.status(of: service.id) ?? .neverRun(service.id)
        }
    }

    /// The most significant state among a Project's Services, or `nil` if it has none.
    func aggregateState(of project: Project, on place: Place = .thisMac) -> ServiceState? {
        .mostSignificant(of: project.services.map { status(of: $0, on: place).state })
    }

    /// Whether its Services can be controlled now: this Mac always, another
    /// Machine while connected.
    func isReachable(_ place: Place) -> Bool {
        switch place {
        case .thisMac: core != nil
        case .machine(let machineID): machineStore?.machine(machineID)?.isConnected ?? false
        }
    }

    var activeServiceCount: Int {
        core?.statuses.statuses.values.filter(\.state.isActive).count ?? 0
    }

    /// The active Services' names by Project, in the sidebar's order, e.g.
    /// to say what an update stops.
    var activeServicesByProject: [(project: String, services: [String])] {
        projects.compactMap { project in
            let names = project.services.filter { status(of: $0).state.isActive }.map(\.name)
            return names.isEmpty ? nil : (project.name, names)
        }
    }

    /// The Projects of `place`: this Mac's, or another Machine's as last shown.
    func projects(on place: Place) -> [Project] {
        switch place {
        case .thisMac: projects
        case .machine(let machineID): shownMachine(machineID)?.projects ?? []
        }
    }

    /// A Key derived from `name` that no other Project of `place` uses.
    func suggestedProjectKey(for name: String, excluding projectID: UUID? = nil, on place: Place = .thisMac) -> String {
        Project.suggestedKey(for: name, among: projects(on: place), excluding: projectID).rawValue
    }

    /// A Key derived from `name` that no other Service of the Project uses.
    func suggestedServiceKey(for name: String, in project: Project, excluding serviceID: UUID? = nil) -> String {
        project.suggestedServiceKey(for: name, excluding: serviceID).rawValue
    }

    // MARK: Lifecycle

    /// Starts the Service; if its port is taken on this Mac, asks what to do
    /// (`blockedStart`). On another Machine, its row says why it Failed.
    func start(_ serviceID: UUID, onPort port: Int? = nil, on place: Place = .thisMac, origin: Origin = .mainWindow) {
        if case .machine(let machineID) = place {
            guard let machines else { return }
            Task { await report(origin: origin) { () throws(LocalhostError) in try await machines.start(serviceID, on: machineID, onPort: port) } }
            return
        }
        guard let services = core?.services else { return }
        Task {
            await report(origin: origin) { () throws(LocalhostError) in
                let status = try await services.start(serviceID, onPort: port, origin: origin)
                await presentIfBlocked(status, origin: origin)
            }
        }
    }

    func stop(_ serviceID: UUID, on place: Place = .thisMac, origin: Origin = .mainWindow) {
        if case .machine(let machineID) = place {
            guard let machines else { return }
            Task { await report(origin: origin) { () throws(LocalhostError) in try await machines.stop(serviceID, on: machineID) } }
            return
        }
        guard let services = core?.services else { return }
        Task { await report(origin: origin) { () throws(LocalhostError) in try await services.stop(serviceID, origin: origin) } }
    }

    func restart(_ serviceID: UUID, on place: Place = .thisMac, origin: Origin = .mainWindow) {
        if case .machine(let machineID) = place {
            guard let machines else { return }
            Task { await report(origin: origin) { () throws(LocalhostError) in try await machines.restart(serviceID, on: machineID) } }
            return
        }
        guard let services = core?.services else { return }
        Task {
            await report(origin: origin) { () throws(LocalhostError) in
                let status = try await services.restart(serviceID, origin: origin)
                await presentIfBlocked(status, origin: origin)
            }
        }
    }

    func startAll(inProject projectID: UUID, on place: Place = .thisMac, origin: Origin = .mainWindow) {
        if case .machine(let machineID) = place {
            guard let machines else { return }
            Task { await report(origin: origin) { () throws(LocalhostError) in try await machines.startAll(inProject: projectID, on: machineID) } }
            return
        }
        guard let services = core?.services else { return }
        Task { await report(origin: origin) { () throws(LocalhostError) in try await services.startAll(inProject: projectID, origin: origin) } }
    }

    func stopAll(inProject projectID: UUID, on place: Place = .thisMac, origin: Origin = .mainWindow) {
        if case .machine(let machineID) = place {
            guard let machines else { return }
            Task { await report(origin: origin) { () throws(LocalhostError) in try await machines.stopAll(inProject: projectID, on: machineID) } }
            return
        }
        guard let services = core?.services else { return }
        Task { await report(origin: origin) { () throws(LocalhostError) in try await services.stopAll(inProject: projectID, origin: origin) } }
    }

    /// The URL "Open" uses for a Service, or `nil` if there is none to open.
    /// On another Machine, its Port is forwarded to the same port here.
    func urlToOpen(_ service: Service, on place: Place = .thisMac) -> URL? {
        service.urlToOpen(for: status(of: service, on: place).run)
    }

    func openURL(of serviceID: UUID, on place: Place = .thisMac) {
        if case .machine(let machineID) = place {
            guard let machines else { return }
            Task {
                await report { () throws(LocalhostError) in
                    if let url = try await machines.openURL(ofService: serviceID, on: machineID) {
                        NSWorkspace.shared.open(url)
                    }
                }
            }
            return
        }
        guard let (_, service) = service(serviceID), let url = urlToOpen(service) else { return }
        NSWorkspace.shared.open(url)
    }

    /// Where the selected Project's logs are read.
    func logs(on place: Place) -> (any LogSource)? {
        switch place {
        case .thisMac: return core?.logs
        case .machine(let machineID): return machines?.logs(on: machineID)
        }
    }

    /// Shows Connect Your Agents, unless another sheet is open: replacing
    /// it would lose its edits.
    func showConnectAgents() {
        guard presentedSheet == nil else { return }
        isShowingPalette = false
        presentedSheet = .connectAgents
    }

    /// Shows the Keyboard Shortcuts sheet (⌘/), or closes it. It doesn't
    /// replace another sheet, which may hold unsaved edits.
    func toggleKeyboardShortcuts() {
        if presentedSheet == .keyboardShortcuts {
            presentedSheet = nil
        } else if presentedSheet == nil {
            isShowingPalette = false
            presentedSheet = .keyboardShortcuts
        }
    }

    // MARK: Logs

    func findInLog() {
        logFindCount += 1
    }

    /// Moves keyboard focus to a column of the main window (see `ColumnNavigation`).
    func focusColumn(_ column: MainColumn) {
        columnFocusRequest = ColumnFocusRequest(column: column)
    }

    /// Whether a log is shown that ⌃L can clear: another Machine's can't be.
    var canClearLogs: Bool {
        selectedPlace == .thisMac && (selectedService != nil || isAllServicesSelected)
    }

    /// Whether a log is shown that ⌘F can filter.
    var canFindInLog: Bool {
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
                try? await logs.clearBuffer(ofService: serviceID)
            }
            logClearCount += 1
        }
    }

    func stopAllServices(origin: Origin = .mainWindow) async {
        await core?.services.stopAll(origin: origin)
    }

    /// Before an update installs: stops every Run, disconnects the coding
    /// agents (the user agreed to both), then retires the shared core, so
    /// the new version starts one of its own.
    func prepareForUpdate() async {
        await core?.services.stopAll(origin: .mainWindow)
        await core?.services.disconnectAgents()
        await core?.services.retireSharedCore()
    }

    /// How many coding agents are connected through `localhost mcp`.
    func connectedAgentCount() async -> Int {
        await core?.services.connectedAgentCount() ?? 0
    }

    // MARK: Editing

    /// Suggestions for a folder being added as a Project, scanned where it is.
    func suggestions(forFolderAt path: String, on place: Place = .thisMac) async throws(LocalhostError) -> [ServiceSuggestion] {
        guard case .machine(let machineID) = place else { return await ProjectScanner().scan(projectAt: path) }
        return try await machines(for: machineID).scan(projectAt: path, on: machineID)
    }

    /// Suggestions for an existing Project, without what its Services cover.
    func suggestions(for project: Project, on place: Place = .thisMac) async throws(LocalhostError) -> [ServiceSuggestion] {
        guard case .machine(let machineID) = place else { return await ProjectScanner().scan(project) }
        return try await machines(for: machineID).scan(project: project.id, on: machineID)
    }

    /// The virtual environment a new Service of the Project would run in, found where the Project is.
    func pythonEnvironment(
        in project: Project, workingDirectory: String, commandLine: String, on place: Place = .thisMac
    ) async -> String? {
        guard case .machine(let machineID) = place else {
            return await ProjectScanner().pythonEnvironment(
                projectPath: project.path, workingDirectory: workingDirectory, commandLine: commandLine
            )
        }
        return try? await machines(for: machineID).pythonEnvironment(
            inProject: project.id, workingDirectory: workingDirectory, commandLine: commandLine, on: machineID
        )
    }

    /// Adds the Project and selects it. Another Machine's arrives once it
    /// reports the change.
    @discardableResult
    func addProject(
        name: String, key: String, path: String, services: [NewService] = [], on place: Place = .thisMac
    ) async throws(LocalhostError) -> Project {
        if case .machine(let machineID) = place {
            let project = try await machines(for: machineID).addProject(name: name, key: Key(key), path: path, services: services, on: machineID)
            sidebarSelection = .machineProject(project.id, machine: machineID)
            return project
        }
        guard let library = core?.library else { throw .ProjectNotFound(UUID()) }
        let project = try await library.addProject(name: name, key: Key(key), path: path, services: services, origin: .mainWindow)
        projects = library.projects()
        selectedProjectID = project.id
        return project
    }

    func editProject(_ id: UUID, name: String, key: String, path: String, on place: Place = .thisMac) async throws(LocalhostError) {
        if case .machine(let machineID) = place {
            try await machines(for: machineID).editProject(id: id, name: name, key: Key(key), path: path, on: machineID)
            return
        }
        guard let library = core?.library else { throw .ProjectNotFound(id) }
        try await library.editProject(id: id, name: name, key: Key(key), path: path, origin: .mainWindow)
    }

    func setFavorite(_ id: UUID, _ isFavorite: Bool, on place: Place = .thisMac) {
        if case .machine(let machineID) = place {
            Task {
                await report { () throws(LocalhostError) in
                    try await machines(for: machineID).editProject(id: id, isFavorite: isFavorite, on: machineID)
                }
            }
            return
        }
        guard let library = core?.library else { return }
        Task { await report { () throws(LocalhostError) in try await library.editProject(id: id, isFavorite: isFavorite, origin: .mainWindow) } }
    }

    /// Stops the Project's Services, then removes it. Another Machine does
    /// both, and deletes their logs, there.
    func removeProject(_ id: UUID, on place: Place = .thisMac) async {
        if case .machine(let machineID) = place {
            await report { () throws(LocalhostError) in try await machines(for: machineID).removeProject(id: id, on: machineID) }
            if sidebarSelection == .machineProject(id, machine: machineID) { sidebarSelection = nil }
            return
        }
        guard let core, let project = project(id) else { return }
        await report { () throws(LocalhostError) in
            for service in project.services {
                try await core.services.stop(service.id, origin: .mainWindow)
            }
            try await core.library.removeProject(id: id, origin: .mainWindow)
            for service in project.services { try await core.logs.removeLogs(ofService: service.id) }
        }
        if selectedProjectID == id { selectedProjectID = projects.first?.id }
    }

    @discardableResult
    func addService(
        to projectID: UUID, name: String, key: String, workingDirectory: String,
        commandLine: String, variables: [VariableInput], dependencies: [UUID], isExcludedFromStartAll: Bool,
        port: Int?, url: String?, portCanChange: Bool, pythonEnvironment: String?, runBefore: String?,
        on place: Place = .thisMac
    ) async throws(LocalhostError) -> Service {
        if case .machine(let machineID) = place {
            let service = try await machines(for: machineID).addService(
                toProject: projectID, name: name, key: Key(key), workingDirectory: workingDirectory,
                commandLine: commandLine, variables: variables, dependencies: dependencies,
                isExcludedFromStartAll: isExcludedFromStartAll, port: port, url: url, portCanChange: portCanChange,
                pythonEnvironment: pythonEnvironment, runBefore: runBefore, on: machineID
            )
            selectedServiceID = service.id
            return service
        }
        guard let library = core?.library else { throw .ProjectNotFound(projectID) }
        let service = try await library.addService(
            toProject: projectID, name: name, key: Key(key), workingDirectory: workingDirectory,
            commandLine: commandLine, variables: variables, dependencies: dependencies,
            isExcludedFromStartAll: isExcludedFromStartAll, port: port, url: url, portCanChange: portCanChange,
            pythonEnvironment: pythonEnvironment, runBefore: runBefore, origin: .mainWindow
        )
        projects = library.projects()
        selectedServiceID = service.id
        return service
    }

    func addServices(_ services: [NewService], to projectID: UUID, on place: Place = .thisMac) async throws(LocalhostError) {
        if case .machine(let machineID) = place {
            let added = try await machines(for: machineID).addServices(services, toProject: projectID, on: machineID)
            if let first = added.first { selectedServiceID = first.id }
            return
        }
        guard let library = core?.library else { throw .ProjectNotFound(projectID) }
        let added = try await library.addServices(services, toProject: projectID, origin: .mainWindow)
        projects = library.projects()
        if let first = added.first { selectedServiceID = first.id }
    }

    /// Saves a reviewed import. On Replace, Services the file no longer has
    /// are stopped first, as when removing them. Another Machine imports
    /// the file's text into the document's folder there.
    func importProject(
        _ document: ProjectDocument, onKeyClash: ProjectLibrary.KeyClash?, on place: Place = .thisMac
    ) async throws(LocalhostError) {
        if case .machine(let machineID) = place {
            let project = try await machines(for: machineID).importProject(
                text: document.yaml, path: document.path, onKeyClash: onKeyClash, on: machineID
            )
            sidebarSelection = .machineProject(project.id, machine: machineID)
            return
        }
        guard let core else { throw .ProjectNotFound(UUID()) }
        if onKeyClash == .replace, let existing = projectClashing(with: document) {
            let keptKeys = Set(document.services.map(\.key))
            for service in existing.services where !keptKeys.contains(service.key) {
                try await core.services.stop(service.id, origin: .mainWindow)
            }
        }
        let replaced = onKeyClash == .replace ? projectClashing(with: document) : nil
        let project = try await core.library.importProject(document, onKeyClash: onKeyClash, origin: .mainWindow)
        for service in replaced?.services ?? [] where !project.services.contains(where: { $0.id == service.id }) {
            try await core.logs.removeLogs(ofService: service.id)
        }
        projects = core.library.projects()
        selectedProjectID = project.id
    }

    func editService(
        _ id: UUID, name: String, key: String, workingDirectory: String,
        commandLine: String, variables: [VariableInput], dependencies: [UUID], isExcludedFromStartAll: Bool,
        port: Int?, url: String?, portCanChange: Bool, pythonEnvironment: String?, runBefore: String?,
        on place: Place = .thisMac
    ) async throws(LocalhostError) {
        if case .machine(let machineID) = place {
            try await machines(for: machineID).editService(
                id: id, name: name, key: Key(key), workingDirectory: workingDirectory,
                commandLine: commandLine, variables: variables, dependencies: dependencies,
                isExcludedFromStartAll: isExcludedFromStartAll, port: .some(port), url: .some(url),
                portCanChange: portCanChange, pythonEnvironment: .some(pythonEnvironment), runBefore: .some(runBefore),
                on: machineID
            )
            return
        }
        guard let library = core?.library else { throw .ServiceNotFound(id) }
        try await library.editService(
            id: id, name: name, key: Key(key), workingDirectory: workingDirectory,
            commandLine: commandLine, variables: variables, dependencies: dependencies,
            isExcludedFromStartAll: isExcludedFromStartAll, port: .some(port), url: .some(url),
            portCanChange: portCanChange, pythonEnvironment: .some(pythonEnvironment), runBefore: .some(runBefore),
            origin: .mainWindow
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

    func setActiveProfile(_ profileID: UUID?, inProject projectID: UUID, on place: Place = .thisMac) {
        if case .machine(let machineID) = place {
            Task {
                await report { () throws(LocalhostError) in
                    try await machines(for: machineID).setActiveProfile(profileID, inProject: projectID, on: machineID)
                }
            }
            return
        }
        guard let library = core?.library else { return }
        Task { await report { () throws(LocalhostError) in try await library.setActiveProfile(profileID, inProject: projectID, origin: .mainWindow) } }
    }

    /// Saves the Environment sheet: the Project's variables, its Profiles
    /// (added, edited, removed), then which one is active. Another Machine
    /// saves it all at once.
    func saveEnvironment(
        of projectID: UUID, variables: [VariableInput], profiles: [ProfileDraft], activeDraftID: UUID?,
        on place: Place = .thisMac
    ) async throws(LocalhostError) {
        if case .machine(let machineID) = place {
            let inputs = profiles.map { draft in
                ProfileInput(profileId: draft.profileID, name: draft.name, variables: draft.variables.compactMap(\.input))
            }
            try await machines(for: machineID).saveEnvironment(
                ofProject: projectID, variables: variables, profiles: inputs,
                activeProfile: profiles.firstIndex { $0.id == activeDraftID }, on: machineID
            )
            return
        }
        guard let library = core?.library, let project = project(projectID) else { throw .ProjectNotFound(projectID) }
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

    /// A Secret's stored value, to reveal it; `nil` if it has none. Another
    /// Machine's is read there each time, never kept here.
    func secretValue(named name: String, in layer: VariableLayer, on place: Place = .thisMac) async -> String? {
        do {
            if case .machine(let machineID) = place {
                return try await machines(for: machineID).secretValue(named: name, in: layer, on: machineID)
            }
            guard let library = core?.library else { return nil }
            return try await library.secretValue(named: name, in: layer)
        } catch {
            actionError = error
            return nil
        }
    }

    /// Stops the Service, then removes it. Another Machine does both, and
    /// deletes its logs, there.
    func removeService(_ id: UUID, on place: Place = .thisMac) async {
        if case .machine(let machineID) = place {
            await report { () throws(LocalhostError) in try await machines(for: machineID).removeService(id: id, on: machineID) }
            if selectedServiceID == id { selectedServiceID = nil }
            return
        }
        guard let core else { return }
        await report { () throws(LocalhostError) in
            try await core.services.stop(id, origin: .mainWindow)
            try await core.library.removeService(id: id, origin: .mainWindow)
            try await core.logs.removeLogs(ofService: id)
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
        guard let core else { return }
        let warnings = PortWarning.warnings(
            for: projects,
            statuses: Array(core.statuses.statuses.values),
            listeners: await core.services.listeners()
        )
        if warnings != portWarnings { portWarnings = warnings }
    }

    /// Why localhost won't stop the Listener, or `nil` if it can.
    func reasonNotStoppable(_ listener: Listener) -> String? {
        listener.reasonNotStoppable?.message
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
        listener.ownerDescription(in: projects)
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
        guard status.state == .failed, case .PortConflict(let port, let holder) = status.run?.failure,
              let services = core?.services
        else { return }
        let nextFreePort = try? await services.nextFreePort(for: status.serviceID)
        blockedStart = BlockedStart(serviceID: status.serviceID, port: Int(port), holder: holder, nextFreePort: nextFreePort)
        // The question is asked in the main window.
        if origin == .menuBar { showMainWindow?() }
    }

    // MARK: Launch

    /// What the user must hear about at launch: Project files that couldn't
    /// be loaded, then Orphans (one alert at a time).
    func runLaunchChecks() async {
        if !hasReportedUnreadableFiles, !(core?.library.unreadableFiles.isEmpty ?? true) {
            isShowingUnreadableFiles = true
            return
        }
        await checkForOrphans()
        offerWelcome()
    }

    /// The telemetry notice was read: it never shows again.
    func acknowledgeTelemetryNotice() {
        isTelemetryNoticePending = false
        telemetry.acknowledgeNotice()
    }

    /// Set once Connect Your Agents had opened by itself, as versions before
    /// the welcome did: a sign the app was opened before.
    private static let hasOfferedAgentsKey = "hasOfferedConnectAgents"

    /// Decides the first run from what was saved, the Projects, and whether
    /// an earlier version was opened; saves what it decided.
    private func startOnboarding() {
        guard let library = core?.library else { return }
        let projectCount = library.projects().count
        let saved = Onboarding.load()
        var onboarding = Onboarding.atLaunch(
            saved: saved,
            hasOpenedBefore: UserDefaults.standard.bool(forKey: Self.hasOfferedAgentsKey),
            projectCount: projectCount
        )
        if !onboarding.hasSeenWelcome, !onboarding.isWelcomeDue(projectCount: projectCount) {
            onboarding.hasSeenWelcome = true
        }
        if onboarding != saved { onboarding.save() }
        self.onboarding = onboarding
    }

    /// Opens the welcome at the first launch, unless a question is up; it
    /// then waits for the next launch.
    private func offerWelcome() {
        guard let onboarding, let library = core?.library,
              onboarding.isWelcomeDue(projectCount: library.projects().count),
              presentedSheet == nil, !isShowingOrphanAlert
        else { return }
        presentedSheet = .welcome
    }

    func updateOnboarding(_ change: (inout Onboarding) -> Void) {
        guard var next = onboarding else { return }
        change(&next)
        guard next != onboarding else { return }
        onboarding = next
        next.save()
    }

    /// The welcome was finished or skipped: it never opens by itself again.
    func finishWelcome(addingProject: Bool) {
        updateOnboarding { $0.hasSeenWelcome = true }
        presentedSheet = addingProject ? .addProject() : nil
    }

    /// Help › Show Welcome…: the first run's welcome on request, Projects or not.
    func showWelcome() {
        guard onboarding != nil, presentedSheet == nil else { return }
        isShowingPalette = false
        presentedSheet = .welcome
    }

    /// Help › Show Get Started Checklist: brings the checklist back after it
    /// was closed or completed. Its rows still tick from what is true.
    func showChecklistAgain() {
        guard onboarding != nil else { return }
        isChecklistReplayed = true
        updateOnboarding { $0.isChecklistDone = false }
    }

    /// The checklist's close button: gone until asked for again.
    func hideChecklist() {
        isChecklistReplayed = false
        updateOnboarding { $0.isChecklistDone = true }
    }

    /// Whether a Service on this Mac is Running now.
    var hasRunningService: Bool {
        core?.statuses.statuses.values.contains { $0.state == .running } ?? false
    }

    var unreadableFilesTitle: String {
        let count = core?.library.unreadableFiles.count ?? 0
        return count == 1 ? "1 project couldn’t be loaded" : "\(count) projects couldn’t be loaded"
    }

    /// One line per file: its name, why, and where it is now.
    var unreadableFilesMessage: String {
        (core?.library.unreadableFiles ?? []).map(\.description).joined(separator: "\n\n")
    }

    /// Closes the report (optionally showing the files), then checks for Orphans.
    func dismissUnreadableFiles(revealing: Bool) {
        hasReportedUnreadableFiles = true
        if revealing, let file = core?.library.unreadableFiles.first {
            NSWorkspace.shared.activateFileViewerSelecting([URL(filePath: file.movedTo ?? file.path)])
        }
        Task {
            await checkForOrphans()
            offerWelcome()
        }
    }

    // MARK: Orphans

    var orphanAlertTitle: String {
        orphans.count == 1
            ? "1 service from the last session is still running"
            : "\(orphans.count) services from the last session are still running"
    }

    /// Asks about the Orphans of a previous session the shared core
    /// reported, now and as it reports more. Call once at launch.
    func checkForOrphans() async {
        isAskingAboutOrphans = true
        presentFoundOrphans()
    }

    private func presentFoundOrphans() {
        guard isAskingAboutOrphans, !foundOrphans.isEmpty, orphans.isEmpty else { return }
        orphans = foundOrphans
        foundOrphans = []
        orphanDescriptions = orphans.map { $0.description(in: projects) }
        isShowingOrphanAlert = true
    }

    func stopOrphans() async {
        guard let services = core?.services else { return }
        isStoppingOrphans = true
        await report { () throws(LocalhostError) in try await services.stopOrphans(orphans) }
        orphans = []
        isStoppingOrphans = false
        presentFoundOrphans()
    }

    func leaveOrphans() {
        guard let services = core?.services else { return }
        let left = orphans
        orphans = []
        Task {
            await report { () throws(LocalhostError) in try await services.leaveOrphans(left) }
            presentFoundOrphans()
        }
    }

    // MARK: Private

    /// Connects to the shared core at launch, starting it if needed, so the
    /// Services it already runs (an agent's, say) show at once.
    private func connectToSharedCore() {
        guard let services = core?.services else { return }
        coreConnection = Task { [weak self] in
            // Subscribed first: the core reports its Orphans as it connects.
            let found = services.foundOrphans()
            do throws(LocalhostError) {
                try await services.connect()
            } catch {
                self?.actionError = error
            }
            for await orphans in found {
                guard let self else { return }
                self.foundOrphans += orphans
                self.presentFoundOrphans()
            }
        }
    }

    private func observeProjects() {
        guard let library = core?.library else { return }
        projectObservation = Task { [weak self] in
            let (projects, events) = library.subscribe()
            self?.projects = projects
            self?.restoreSelection()
            for await event in events {
                guard let self else { return }
                switch event {
                case .projectAdded, .projectUpdated, .projectRemoved:
                    self.projects = library.projects()
                }
            }
        }
    }

    /// Has VoiceOver say when a Service settles (Running, Exited, Failed or
    /// Stopped), wherever it is, since only its row shows it otherwise. The
    /// states in between are in the rows' values, and would be noise here.
    private func announceStates() {
        guard let services = core?.services else { return }
        stateAnnouncements = Task { [weak self] in
            var lastStates: [UUID: ServiceState] = [:]
            for await event in services.subscribe().events {
                guard let self, case .serviceStateChanged(let status) = event else { continue }
                defer { lastStates[status.serviceID] = status.state }
                guard status.state != lastStates[status.serviceID], !status.state.isActive || status.state == .running,
                      NSWorkspace.shared.isVoiceOverEnabled,
                      let (project, service) = self.service(status.serviceID)
                else { continue }
                let subject = project.id == self.selectedProjectID ? service.name : "\(project.name), \(service.name)"
                var announcement = AttributedString("\(subject), \(status.spokenSummary)")
                announcement.accessibilitySpeechAnnouncementPriority = status.state == .failed ? .high : .default
                AccessibilityNotification.Announcement(announcement).post()
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
        let project = savedSidebar.flatMap(UUID.init(uuidString:)).flatMap { self.project($0) } ?? projects[0]
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
        // Another Machine's Projects arrive only once it connects.
        case .machineProject, nil: defaults.removeObject(forKey: Self.sidebarSelectionKey)
        }
        defaults.set(selectedServiceID?.uuidString, forKey: Self.serviceSelectionKey)
    }

    func report(origin: Origin = .mainWindow, _ action: () async throws(LocalhostError) -> Void) async {
        do {
            try await action()
        } catch {
            actionError = error
            // Errors are reported in the main window.
            if origin == .menuBar { showMainWindow?() }
        }
    }
}
