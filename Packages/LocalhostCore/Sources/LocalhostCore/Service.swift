import Foundation

/// One independently manageable long-running command belonging to a Project.
///
/// The command line is run through the user's login shell (ADR-0003), so
/// there is no separate arguments field.
public struct Service: Identifiable, Hashable, Sendable, Codable {
    public let id: UUID
    public internal(set) var name: String
    public internal(set) var key: Key
    /// Relative to the Project's path; `.` is the Project root.
    public internal(set) var workingDirectory: String
    public internal(set) var commandLine: String
    /// Plain variables. Secrets are listed in `secretNames`.
    public internal(set) var environment: [String: String]
    /// The Service's Secrets, sorted; their values are in the `SecretStore`.
    public internal(set) var secretNames: [String]
    /// The Services of the same Project this one needs before it can start.
    public internal(set) var dependencies: [UUID]
    /// Start All skips this Service, unless a Service it starts depends on it.
    public internal(set) var isExcludedFromStartAll: Bool
    /// The TCP port the Service listens on: it is Running once the Port
    /// accepts connections, and its command gets it as `PORT` (ADR-0006).
    public internal(set) var port: Int?
    /// Where to open the Service, e.g. `http://localhost:3000/admin`.
    public internal(set) var url: String?
    /// Whether a Run may use another port when the Port is taken. Off for
    /// commands that ignore `$PORT` (ADR-0006).
    public internal(set) var portCanChange: Bool

    init(
        id: UUID = UUID(),
        name: String,
        key: Key,
        workingDirectory: String,
        commandLine: String,
        environment: [String: String] = [:],
        secretNames: [String] = [],
        dependencies: [UUID] = [],
        isExcludedFromStartAll: Bool = false,
        port: Int? = nil,
        url: String? = nil,
        portCanChange: Bool = true
    ) {
        self.id = id
        self.name = name
        self.key = key
        self.workingDirectory = workingDirectory
        self.commandLine = commandLine
        self.environment = environment
        self.secretNames = secretNames
        self.dependencies = dependencies
        self.isExcludedFromStartAll = isExcludedFromStartAll
        self.port = port
        self.url = url
        self.portCanChange = portCanChange
    }

    private enum CodingKeys: String, CodingKey {
        case id, name, key, workingDirectory, commandLine, environment, secretNames, dependencies, isExcludedFromStartAll, port, url, portCanChange
    }

    /// Fields added after schema version 1 shipped are optional when reading.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        key = try container.decode(Key.self, forKey: .key)
        workingDirectory = try container.decode(String.self, forKey: .workingDirectory)
        commandLine = try container.decode(String.self, forKey: .commandLine)
        environment = try container.decode([String: String].self, forKey: .environment)
        secretNames = try container.decodeIfPresent([String].self, forKey: .secretNames) ?? []
        dependencies = try container.decodeIfPresent([UUID].self, forKey: .dependencies) ?? []
        isExcludedFromStartAll = try container.decodeIfPresent(Bool.self, forKey: .isExcludedFromStartAll) ?? false
        port = try container.decodeIfPresent(Int.self, forKey: .port)
        url = try container.decodeIfPresent(String.self, forKey: .url)
        portCanChange = try container.decodeIfPresent(Bool.self, forKey: .portCanChange) ?? true
    }

    /// The URL to open for this Service while `run` is its latest Run: the
    /// configured URL with its port moved to the Run's, else `localhost` at
    /// the Run's port, else the first local URL the Run printed.
    /// The Service's layer of a Run's environment.
    public var variables: Variables {
        get { Variables(values: environment, secretNames: secretNames) }
        set { (environment, secretNames) = (newValue.values, newValue.secretNames) }
    }

    /// Whether a command line mentions `$PORT`, so a moved port reaches it.
    public static func commandLineUsesPort(_ commandLine: String) -> Bool {
        commandLine.contains("$PORT") || commandLine.contains("${PORT")
    }

    public func urlToOpen(for run: Run?) -> URL? {
        let actualPort = run?.port ?? port
        if let url, var components = URLComponents(string: url) {
            if let port, let actualPort, components.port == port { components.port = actualPort }
            return components.url
        }
        if let actualPort { return URL(string: "http://localhost:\(actualPort)") }
        return run?.detectedURL
    }
}
