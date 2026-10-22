import Foundation

// Identity, Foundation types and methods on the core's value types. Logic
// stays in the core; these forward to its exported functions.

// MARK: Projects

extension Project: Identifiable {
    public var activeProfileID: UUID? { activeProfileId }

    public var activeProfile: EnvironmentProfile? {
        activeProfileId.flatMap { profile(id: $0) }
    }

    public func profile(id: UUID) -> EnvironmentProfile? {
        profiles.first { $0.id == id }
    }

    public func service(id: UUID) -> Service? {
        services.first { $0.id == id }
    }

    /// The Service's working directory, resolved against the Project folder.
    public func workingDirectoryURL(for service: Service) -> URL {
        URL(corePath: projectWorkingDirectory(project: self, service: service), isDirectory: true)
    }

    /// The Services that depend on `serviceID`, directly.
    public func dependents(of serviceID: UUID) -> [Service] {
        projectDependents(project: self, serviceId: serviceID)
    }

    /// The cycle making `serviceID` depend on `dependencyID` would close, as
    /// Keys from `serviceID` back to it; `nil` if there is none.
    public func dependencyCycle(from serviceID: UUID, to dependencyID: UUID) -> [Key]? {
        projectDependencyCycle(project: self, serviceId: serviceID, dependencyId: dependencyID)
    }

    /// `serviceIDs` and everything they depend on, dependencies first.
    public func dependencyOrder(of serviceIDs: [UUID]) -> [UUID] {
        projectDependencyOrder(project: self, serviceIds: serviceIDs)
    }

    /// A Key derived from `name` that no other Service of the Project uses.
    public func suggestedServiceKey(for name: String, excluding serviceID: UUID? = nil) -> Key {
        projectSuggestedServiceKey(project: self, name: name, excluding: serviceID)
    }

    /// A Key derived from `name` that none of `projects` uses but the one being edited.
    public static func suggestedKey(for name: String, among projects: [Project], excluding projectID: UUID? = nil) -> Key {
        suggestedProjectKey(projects: projects, name: name, excluding: projectID)
    }
}

extension EnvironmentProfile: Identifiable {}

// MARK: Services

extension Service: Identifiable {
    public var variables: Variables {
        Variables(values: environment, secretNames: secretNames)
    }

    /// Whether the command line reads `$PORT`.
    public static func commandLineUsesPort(_ commandLine: String) -> Bool {
        LocalhostCore.commandLineUsesPort(commandLine: commandLine)
    }

    /// The address to open: the Service's URL, or `http://localhost:<port>`,
    /// on the port the Run actually got.
    public func urlToOpen(for run: Run?) -> URL? {
        serviceUrlToOpen(service: self, run: run).flatMap(URL.init(string:))
    }
}

extension NewService {
    public init(
        name: String,
        key: Key,
        workingDirectory: String = ".",
        commandLine: String,
        port: Int? = nil,
        url: String? = nil,
        portCanChange: Bool = true,
        isExcludedFromStartAll: Bool = false
    ) {
        self.init(
            name: name,
            key: key,
            workingDirectory: workingDirectory,
            commandLine: commandLine,
            port: port.map(Int64.init),
            url: url,
            portCanChange: portCanChange,
            isExcludedFromStartAll: isExcludedFromStartAll
        )
    }

    /// A Service to add from a reviewed Suggestion.
    public init(_ suggestion: ServiceSuggestion) {
        self.init(
            name: suggestion.name,
            key: suggestion.key,
            workingDirectory: suggestion.workingDirectory,
            commandLine: suggestion.commandLine,
            port: suggestion.port,
            portCanChange: suggestion.portCanChange
        )
    }
}

extension ServiceSuggestion: Identifiable {
    public var id: String { "\(workingDirectory)\u{0}\(commandLine)" }
}

// MARK: Variables

extension Variables {
    public static let empty = Variables(values: [:], secretNames: [])

    public var isEmpty: Bool { values.isEmpty && secretNames.isEmpty }

    /// Every name, plain and Secret, sorted.
    public var names: [String] { variablesNames(variables: self) }
}

extension VariableInput {
    public typealias Value = VariableValue

    public static func plain(_ name: String, _ value: String) -> VariableInput {
        VariableInput(name: name, value: .plain(value))
    }

    public static func secret(_ name: String, _ value: String) -> VariableInput {
        VariableInput(name: name, value: .secret(value))
    }

    /// Keeps a stored Secret, renamed from `previousName` if given.
    public static func existingSecret(_ name: String, previousName: String? = nil) -> VariableInput {
        VariableInput(name: name, value: .existingSecret(previousName: previousName ?? name))
    }

    public static func plain(_ values: [String: String]) -> [VariableInput] {
        values.sorted { $0.key < $1.key }.map { .plain($0.key, $0.value) }
    }
}

// MARK: Runs and States

extension ServiceState {
    /// Starting, Running or Stopping: a process may be alive.
    public var isActive: Bool { serviceStateIsActive(state: self) }

    /// The state that says most about a group of Services (Failed first), or
    /// `nil` for none.
    public static func mostSignificant(of states: [ServiceState]) -> ServiceState? {
        serviceStateMostSignificant(states: states)
    }
}

extension Run: Identifiable {
    public var serviceID: UUID { serviceId }

    /// The local URL the Service printed, if any.
    public var detectedURL: URL? { detectedUrl.flatMap(URL.init(string:)) }
}

extension PortChange {
    public typealias Reason = PortChangeReason
}

extension ServiceStatus {
    public var serviceID: UUID { serviceId }

    public static func neverRun(_ serviceID: UUID) -> ServiceStatus {
        serviceStatusNeverRun(serviceId: serviceID)
    }
}

extension ServiceEvent {
    public var serviceID: UUID {
        switch self {
        case .serviceStateChanged(let status): status.serviceId
        case .runStarted(let run), .runEnded(let run): run.serviceId
        }
    }
}

extension Termination {
    /// "Exited with code 1", "Terminated by signal SIGTERM".
    public var summary: String { terminationSummary(termination: self) }

    /// `SIGTERM` for 15; the number for signals without a common name.
    public static func signalName(_ signal: Int32) -> String {
        LocalhostCore.signalName(signal: signal)
    }
}

// MARK: Output

extension OutputLine {
    public typealias Channel = OutputChannel
}

extension ServiceOutputLine {
    public var serviceID: UUID { serviceId }
}

// MARK: Listeners and Orphans

extension Listener: Identifiable {
    public var id: Int32 { pid }

    public var processGroupID: Int32 { processGroupId }

    /// Not started by localhost.
    public var isUnmanaged: Bool { listenerIsUnmanaged(listener: self) }

    /// Why localhost won't stop it, or `nil` if it can.
    public var reasonNotStoppable: LocalhostError? { listenerReasonNotStoppable(listener: self) }

    /// Who it belongs to, e.g. "My SaaS › API", "In My SaaS", "Docker", "Unmanaged".
    public func ownerDescription(in projects: [Project]) -> String {
        listenerOwnerDescription(listener: self, projects: projects)
    }
}

extension PortWarning: Identifiable {
    public var id: String { "\(serviceId)-\(corePort)" }
    public var serviceID: UUID { serviceId }

    /// The configured Ports of `projects` held by Unmanaged Listeners, leaving
    /// out active Services.
    public static func warnings(for projects: [Project], statuses: [ServiceStatus], listeners: [Listener]) -> [PortWarning] {
        portWarnings(projects: projects, statuses: statuses, listeners: listeners)
    }
}

extension Orphan: Identifiable {
    public var id: UUID { runId }
    public var runID: UUID { runId }
    public var serviceID: UUID { serviceId }
    public var processGroupID: Int32 { processGroupId }

    /// "My SaaS › API", or which process group, if its Service is gone.
    public func description(in projects: [Project]) -> String {
        orphanDescription(orphan: self, projects: projects)
    }
}

extension UnreadableProjectFile {
    /// The file's name, why it couldn't be loaded, and where it is now.
    public var description: String { unreadableProjectFileDescription(file: self) }
}

// MARK: Ports

// The core's ports are u16; Swift code works in Int.

extension Service {
    public var port: Int? { corePort.map(Int.init) }
}

extension Run {
    /// The port this Run was given, which may differ from the Service's.
    public var port: Int? { corePort.map(Int.init) }
}

extension Listener {
    public var ports: [Int] { corePorts.map(Int.init) }
}

extension PortChange {
    public var configuredPort: Int { Int(coreConfiguredPort) }
}

extension ServiceSuggestion {
    public var port: Int? { corePort.map(Int.init) }
}

extension DocumentService {
    public var port: Int? { corePort.map(Int.init) }
}

extension PortWarning {
    public var port: Int { Int(corePort) }
}
