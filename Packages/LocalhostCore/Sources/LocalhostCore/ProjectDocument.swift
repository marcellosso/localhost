import Foundation

/// A Project's Configuration as a Project file: one YAML file, written by
/// Export and read by Import, so it can move between Macs (and later into a
/// repo or a CLI). It has no UUIDs, sort order or favourite, and Secrets are
/// listed by name only: their values never leave the `SecretStore`.
///
/// A document read from YAML is validated with the editors' rules, so
/// `ProjectLibrary.importProject` only fails on what changed since: the
/// path the user picked, or a Key that became taken.
public struct ProjectDocument: Hashable, Sendable {
    /// The `localhost:` value of files this version writes and reads.
    public static let formatVersion = 1

    public struct Service: Hashable, Sendable {
        public internal(set) var name: String
        public internal(set) var key: Key
        /// Relative to the Project's path; `.` is the Project root.
        public internal(set) var workingDirectory: String
        public internal(set) var commandLine: String
        public internal(set) var port: Int?
        public internal(set) var url: String?
        public internal(set) var portCanChange: Bool
        public internal(set) var isExcludedFromStartAll: Bool
        /// The Keys of the Services of the same file it needs, in order.
        public internal(set) var dependencies: [Key]
        public internal(set) var variables: Variables
    }

    public struct Profile: Hashable, Sendable {
        public internal(set) var name: String
        public internal(set) var variables: Variables
    }

    public internal(set) var name: String
    public internal(set) var key: Key
    /// The absolute path of the Project's folder. Import may point it elsewhere,
    /// e.g. when the folder is somewhere else on this Mac.
    public var path: String
    public internal(set) var variables: Variables
    public internal(set) var services: [Service]
    public internal(set) var profiles: [Profile]
    /// The name of the active Environment Profile, if any.
    public internal(set) var activeProfile: String?

    public init(_ project: Project) {
        name = project.name
        key = project.key
        path = project.path
        variables = project.variables
        services = project.services.map { service in
            Service(
                name: service.name, key: service.key, workingDirectory: service.workingDirectory,
                commandLine: service.commandLine, port: service.port, url: service.url,
                portCanChange: service.portCanChange, isExcludedFromStartAll: service.isExcludedFromStartAll,
                dependencies: service.dependencies.compactMap { project.service(id: $0)?.key },
                variables: service.variables
            )
        }
        profiles = project.profiles.map { Profile(name: $0.name, variables: $0.variables) }
        activeProfile = project.activeProfile?.name
    }

    // MARK: Writing

    /// The Project file. Fields with their default value are left out, and
    /// variables are sorted, so the same Configuration always gives the same text.
    public var yaml: String {
        var lines = [
            "localhost: \(Self.formatVersion)",
            "name: \(YAMLScalar.write(name))",
            "key: \(YAMLScalar.write(key.rawValue))",
            "path: \(YAMLScalar.write((path as NSString).abbreviatingWithTildeInPath))",
        ]
        lines += Self.lines(of: variables, indent: "")
        if !services.isEmpty {
            lines.append("services:")
            for service in services {
                lines.append("  \(YAMLScalar.write(service.key.rawValue)):")
                lines.append("    name: \(YAMLScalar.write(service.name))")
                lines.append("    command: \(YAMLScalar.write(service.commandLine))")
                lines.append("    cwd: \(YAMLScalar.write(service.workingDirectory))")
                if let port = service.port { lines.append("    port: \(port)") }
                if let url = service.url { lines.append("    url: \(YAMLScalar.write(url))") }
                if !service.portCanChange { lines.append("    port_can_change: false") }
                if service.isExcludedFromStartAll { lines.append("    start_all: false") }
                if !service.dependencies.isEmpty {
                    lines.append("    depends_on:")
                    lines += service.dependencies.map { "      - \(YAMLScalar.write($0.rawValue))" }
                }
                lines += Self.lines(of: service.variables, indent: "    ")
            }
        }
        if !profiles.isEmpty {
            lines.append("profiles:")
            for profile in profiles {
                let variableLines = Self.lines(of: profile.variables, indent: "    ")
                lines.append("  \(YAMLScalar.write(profile.name)):" + (variableLines.isEmpty ? " {}" : ""))
                lines += variableLines
            }
        }
        if let activeProfile { lines.append("active_profile: \(YAMLScalar.write(activeProfile))") }
        return lines.joined(separator: "\n") + "\n"
    }

    private static func lines(of variables: Variables, indent: String) -> [String] {
        var lines: [String] = []
        if !variables.values.isEmpty {
            lines.append("\(indent)variables:")
            lines += variables.values.sorted { $0.key < $1.key }
                .map { "\(indent)  \(YAMLScalar.write($0.key)): \(YAMLScalar.write($0.value))" }
        }
        if !variables.secretNames.isEmpty {
            lines.append("\(indent)secrets:")
            lines += variables.secretNames.map { "\(indent)  - \(YAMLScalar.write($0))" }
        }
        return lines
    }

    // MARK: Reading

    /// Reads and validates a Project file. Errors are
    /// `LocalhostError.invalidProjectFile`, naming the field and line.
    public init(yaml text: String) throws(LocalhostError) {
        let root = try YAMLValue.parse(text)
        guard case .mapping(let entries) = root.kind, let version = entries.first(where: { $0.key == "localhost" }) else {
            throw .invalidProjectFile(.notAProjectFile, field: [], line: nil)
        }
        guard case .scalar(let raw) = version.value.kind, let number = Int(raw), number >= 1 else {
            throw .invalidProjectFile(.expected(.number), field: ["localhost"], line: version.line)
        }
        guard number <= Self.formatVersion else {
            throw .invalidProjectFile(.newerFormat(number), field: ["localhost"], line: version.line)
        }

        var file = try Fields(root, field: [])
        _ = file.entry("localhost")
        name = try file.required("name") { (raw, _) throws(LocalhostError) in try Validate.name(raw) }
        key = try file.required("key") { (raw, _) throws(LocalhostError) in try Key(raw) }
        path = try file.required("path") { (raw, _) throws(LocalhostError) in try Validate.projectPath(raw) }
        variables = try Self.variables(in: &file)

        var services: [Service] = []
        var dependencyLines: [[(raw: String, line: Int)]] = []
        if let entry = file.entry("services") {
            for serviceEntry in try Fields(entry.value, field: ["services"], line: entry.line).entries {
                let (service, dependencies) = try Self.service(serviceEntry)
                services.append(service)
                dependencyLines.append(dependencies)
            }
        }
        try Self.resolveDependencies(dependencyLines, of: &services)
        self.services = services

        var profiles: [Profile] = []
        if let entry = file.entry("profiles") {
            for profileEntry in try Fields(entry.value, field: ["profiles"], line: entry.line).entries {
                let field = ["profiles", profileEntry.key]
                let name = try Fields.check(field, profileEntry.line) { () throws(LocalhostError) in
                    let name = try Validate.name(profileEntry.key)
                    if profiles.contains(where: { $0.name == name }) { throw .duplicateProfileName(name) }
                    return name
                }
                var fields = try Fields(profileEntry.value, field: field, line: profileEntry.line)
                profiles.append(Profile(name: name, variables: try Self.variables(in: &fields)))
                try fields.finish()
            }
        }
        self.profiles = profiles

        activeProfile = try file.optional("active_profile") { (raw, _) throws(ProjectFileProblem) in
            let name = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard profiles.contains(where: { $0.name == name }) else { throw .unknownProfile(name) }
            return name
        }
        try file.finish()
    }

    private static func service(_ entry: YAMLValue.Entry) throws(LocalhostError) -> (Service, [(raw: String, line: Int)]) {
        let key = try Fields.check(["services", entry.key], entry.line) { () throws(LocalhostError) in try Key(entry.key) }
        var fields = try Fields(entry.value, field: ["services", entry.key], line: entry.line)
        let service = Service(
            name: try fields.optional("name") { (raw, _) throws(LocalhostError) in try Validate.name(raw) } ?? entry.key,
            key: key,
            workingDirectory: try fields.optional("cwd") { (raw, _) throws(LocalhostError) in try Validate.workingDirectory(raw) } ?? ".",
            commandLine: try fields.required("command") { (raw, _) throws(LocalhostError) in try Validate.commandLine(raw) },
            port: try fields.optional("port") { (raw, _) throws(ProjectFileProblem) in
                guard let port = Int(raw) else { throw .expected(.number) }
                do throws(LocalhostError) { _ = try Validate.port(port) } catch { throw .invalid(error) }
                return port
            },
            url: try fields.optional("url") { (raw, _) throws(LocalhostError) in try Validate.url(raw) } ?? nil,
            portCanChange: try fields.boolean("port_can_change") ?? true,
            isExcludedFromStartAll: !(try fields.boolean("start_all") ?? true),
            dependencies: [],
            variables: try variables(in: &fields)
        )
        let dependencies = try fields.list("depends_on")
        try fields.finish()
        return (service, dependencies)
    }

    /// Turns the `depends_on` Keys into Dependencies, rejecting unknown Keys
    /// and cycles as the Service editor does. Repeated Keys are dropped.
    private static func resolveDependencies(_ lists: [[(raw: String, line: Int)]], of services: inout [Service]) throws(LocalhostError) {
        // A scratch Project to reuse the editor's cycle check.
        var project = Project(name: "", key: try Key("scratch"), path: "/", sortOrder: 0, services: services.map {
            ConfiguredService(name: $0.name, key: $0.key, workingDirectory: $0.workingDirectory, commandLine: $0.commandLine)
        })
        for (index, list) in lists.enumerated() {
            let field = ["services", services[index].key.rawValue, "depends_on"]
            for (raw, line) in list {
                guard let dependency = project.services.first(where: { $0.key.rawValue == raw }) else {
                    throw .invalidProjectFile(.unknownDependency(raw), field: field, line: line)
                }
                guard !project.services[index].dependencies.contains(dependency.id) else { continue }
                if let cycle = project.dependencyCycle(from: project.services[index].id, to: dependency.id) {
                    throw .invalidProjectFile(.invalid(.dependencyCycle(cycle)), field: field, line: line)
                }
                project.services[index].dependencies.append(dependency.id)
                services[index].dependencies.append(dependency.key)
            }
        }
    }

    /// A layer's `variables` (name → value) and `secrets` (names).
    private static func variables(in fields: inout Fields) throws(LocalhostError) -> Variables {
        var values: [String: String] = [:]
        if let entry = fields.entry("variables") {
            let variables = try Fields(entry.value, field: fields.field + ["variables"], line: entry.line)
            for variable in variables.entries {
                let field = variables.field + [variable.key]
                guard Validate.isValidVariableName(variable.key) else {
                    throw .invalidProjectFile(.invalid(.invalidEnvironmentVariableName(variable.key)), field: field, line: variable.line)
                }
                switch variable.value.kind {
                case .scalar(let value): values[variable.key] = value
                case .null: values[variable.key] = ""
                case .sequence, .mapping: throw .invalidProjectFile(.expected(.text), field: field, line: variable.line)
                }
            }
        }
        var secretNames: [String] = []
        for (name, line) in try fields.list("secrets") {
            let problem: LocalhostError? = if !Validate.isValidVariableName(name) {
                .invalidEnvironmentVariableName(name)
            } else if values[name] != nil || secretNames.contains(name) {
                .duplicateVariableName(name)
            } else {
                nil
            }
            if let problem { throw .invalidProjectFile(.invalid(problem), field: fields.field + ["secrets"], line: line) }
            secretNames.append(name)
        }
        return Variables(values: values, secretNames: secretNames.sorted())
    }
}

/// The fields of one mapping of a Project file, read once each; whatever is
/// left when it is finished is an unknown field.
private struct Fields {
    let field: [String]
    let line: Int
    let entries: [YAMLValue.Entry]
    private var used: Set<String> = []

    /// `null` (e.g. `Testing:` with nothing under it) reads as no fields.
    init(_ value: YAMLValue, field: [String], line: Int? = nil) throws(LocalhostError) {
        self.field = field
        self.line = line ?? value.line
        switch value.kind {
        case .mapping(let entries): self.entries = entries
        case .null: entries = []
        case .scalar, .sequence: throw .invalidProjectFile(.expected(.mapping), field: field, line: self.line)
        }
    }

    mutating func entry(_ key: String) -> YAMLValue.Entry? {
        guard let entry = entries.first(where: { $0.key == key }) else { return nil }
        used.insert(key)
        return entry
    }

    /// A text field's value, transformed (and validated) by `transform`.
    mutating func optional<T, Failure: Error>(
        _ key: String, _ transform: (String, Int) throws(Failure) -> T
    ) throws(LocalhostError) -> T? {
        guard let entry = entry(key) else { return nil }
        guard case .scalar(let raw) = entry.value.kind else {
            throw .invalidProjectFile(.expected(.text), field: field + [key], line: entry.line)
        }
        do {
            return try transform(raw, entry.line)
        } catch let problem as ProjectFileProblem {
            throw .invalidProjectFile(problem, field: field + [key], line: entry.line)
        } catch let error as LocalhostError {
            throw .invalidProjectFile(.invalid(error), field: field + [key], line: entry.line)
        } catch {
            preconditionFailure("Unexpected \(error)")
        }
    }

    mutating func required<T, Failure: Error>(
        _ key: String, _ transform: (String, Int) throws(Failure) -> T
    ) throws(LocalhostError) -> T {
        guard let value = try optional(key, transform) else {
            throw .invalidProjectFile(.missing, field: field + [key], line: line)
        }
        return value
    }

    mutating func boolean(_ key: String) throws(LocalhostError) -> Bool? {
        try optional(key) { (raw, _) throws(ProjectFileProblem) in
            switch raw {
            case "true": return true
            case "false": return false
            default: throw .expected(.boolean)
            }
        }
    }

    /// A list of text items with their lines; `[]` when absent or empty.
    mutating func list(_ key: String) throws(LocalhostError) -> [(raw: String, line: Int)] {
        guard let entry = entry(key) else { return [] }
        switch entry.value.kind {
        case .null:
            return []
        case .sequence(let items):
            var result: [(raw: String, line: Int)] = []
            for item in items {
                guard case .scalar(let raw) = item.kind else {
                    throw .invalidProjectFile(.expected(.text), field: field + [key], line: item.line)
                }
                result.append((raw, item.line))
            }
            return result
        case .scalar, .mapping:
            throw .invalidProjectFile(.expected(.list), field: field + [key], line: entry.line)
        }
    }

    func finish() throws(LocalhostError) {
        if let unknown = entries.first(where: { !used.contains($0.key) }) {
            throw .invalidProjectFile(.unknownField, field: field + [unknown.key], line: unknown.line)
        }
    }

    /// Runs `body`, reporting its error as the field's.
    static func check<T>(_ field: [String], _ line: Int, _ body: () throws(LocalhostError) -> T) throws(LocalhostError) -> T {
        do {
            return try body()
        } catch {
            throw .invalidProjectFile(.invalid(error), field: field, line: line)
        }
    }
}

/// The core's Service, where `ProjectDocument.Service` shadows it.
private typealias ConfiguredService = Service

/// What is wrong in a Project file; see `LocalhostError.invalidProjectFile`.
public enum ProjectFileProblem: Error, Sendable, Hashable, Codable {
    public enum ValueKind: Sendable, Hashable, Codable {
        case text, number, boolean, list, mapping
    }

    /// The file has no `localhost:` version line.
    case notAProjectFile
    /// Written by a newer localhost, in this format version.
    case newerFormat(Int)
    case expectedKeyValue
    case unexpectedIndentation
    case unterminatedQuote
    case invalidEscape
    /// YAML this reader doesn't take, e.g. "anchors and aliases".
    case unsupportedYAML(String)
    case duplicate
    case missing
    case unknownField
    case expected(ValueKind)
    /// A `depends_on` Key that no Service of the file has.
    case unknownDependency(String)
    /// An `active_profile` that names no Profile of the file.
    case unknownProfile(String)
    /// The value breaks an editor's rule (Key, port, URL, cycle, …).
    indirect case invalid(LocalhostError)

    public var message: String {
        switch self {
        case .notAProjectFile: "This isn’t a localhost project file: it has no “localhost: 1” line."
        case .newerFormat(let version): "This file was written by a newer version of localhost (format \(version))."
        case .expectedKeyValue: "Expected “key: value” or “- item”."
        case .unexpectedIndentation: "The indentation doesn’t match the lines around it."
        case .unterminatedQuote: "A quoted value isn’t closed."
        case .invalidEscape: "A double-quoted value has an unknown escape sequence."
        case .unsupportedYAML(let feature): "localhost project files don’t support \(feature)."
        case .duplicate: "Defined more than once."
        case .missing: "This field is required."
        case .unknownField: "localhost doesn’t know this field."
        case .expected(.text): "Expected text."
        case .expected(.number): "Expected a whole number."
        case .expected(.boolean): "Expected true or false."
        case .expected(.list): "Expected a list."
        case .expected(.mapping): "Expected “key: value” lines."
        case .unknownDependency(let key): "No service in this file has the key “\(key)”."
        case .unknownProfile(let name): "No profile in this file is called “\(name)”."
        case .invalid(let error): error.message
        }
    }
}
