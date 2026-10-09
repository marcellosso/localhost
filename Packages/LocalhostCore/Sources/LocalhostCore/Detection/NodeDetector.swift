import Foundation

/// Suggests a Service for each `package.json` script, run with the
/// package manager the lockfiles point to.
///
/// Every script is suggested; one long-running script per package (`dev`,
/// else `develop`, `start`, `serve`, `watch`) is checked by default. In a
/// workspace monorepo the packages are checked rather than the root's
/// orchestrating scripts (`turbo dev`), since localhost runs each package as
/// its own Service. When the script runs a known dev server, its default port
/// is suggested and, if the tool takes a port flag, `$PORT` is passed to it
/// (ADR-0006).
public struct NodeDetector: ServiceDetector {
    public init() {}

    public func declaredDirectories(in root: ScannedDirectory) -> [String] {
        NodeWorkspaces(root: root).packagePaths
    }

    public func suggestions(in directory: ScannedDirectory) -> [ServiceSuggestion] {
        guard let manifest = directory.read("package.json").flatMap(PackageManifest.init),
              !manifest.scripts.isEmpty
        else { return [] }
        let rootDirectory = directory.selfAndAncestors.last!
        let workspaces = NodeWorkspaces(root: rootDirectory)
        let isWorkspaceRoot = directory.isRoot && !workspaces.packagePaths.isEmpty
        let isOutsideWorkspaces = !directory.isRoot && !workspaces.patterns.isEmpty
            && !workspaces.packagePaths.contains(directory.path)
        let packageManager = PackageManager.detect(for: directory)

        let scripts = manifest.scripts.filter { !Self.isLifecycleScript($0.name, among: manifest.scripts) }
        let primary = Self.primaryScript(among: scripts.map(\.name))
        let baseName = manifest.unscopedName ?? (directory.isRoot ? nil : directory.name)

        let suggestions = scripts.map { script in
            let isPrimary = script.name == primary
            let name = switch (isPrimary, baseName) {
            case (true, let base?): base
            case (_, let base?): "\(base) \(script.name)"
            case (_, nil): script.name
            }
            let server = DevServer.recognize(script.body)
            var commandLine = packageManager.run(script.name)
            if let flag = server?.portFlag {
                commandLine += packageManager.forwardingArguments + " \(flag) $PORT"
            }
            return ServiceSuggestion(
                name: name,
                key: .suggestion(name),
                workingDirectory: directory.path,
                commandLine: commandLine,
                port: server?.port,
                portCanChange: server?.canMove ?? true,
                isRecommended: isPrimary && !isWorkspaceRoot && !isOutsideWorkspaces,
                foundIn: directory.relativePath("package.json"),
                detail: "\"\(script.name)\": \(script.body)"
            )
        }
        return suggestions.filter(\.isRecommended) + suggestions.filter { !$0.isRecommended }
    }

    /// Scripts npm runs around installing and publishing, and `pre`/`post`
    /// hooks of other scripts; never Services.
    static func isLifecycleScript(_ name: String, among scripts: [PackageManifest.Script]) -> Bool {
        let lifecycle: Set = [
            "preinstall", "install", "postinstall", "prepare", "prepublish", "prepublishOnly",
            "prepack", "postpack", "publish", "postpublish", "preversion", "version", "postversion",
        ]
        if lifecycle.contains(name) { return true }
        for prefix in ["pre", "post"] where name.hasPrefix(prefix) {
            let hooked = name.dropFirst(prefix.count)
            if scripts.contains(where: { $0.name == hooked }) { return true }
        }
        return false
    }

    /// The script checked by default: the first of `dev`, `develop`, `start`,
    /// `serve`, `watch` present by exact name, else the first `dev:…`-style one.
    static func primaryScript(among names: [String]) -> String? {
        let preferred = ["dev", "develop", "start", "serve", "watch"]
        if let exact = preferred.first(where: names.contains) { return exact }
        return names.first { name in
            preferred.contains { name.hasPrefix($0 + ":") || name.hasPrefix($0 + "-") }
        }
    }
}

/// The parts of a `package.json` detection reads.
struct PackageManifest {
    struct Script: Equatable {
        var name: String
        var body: String
    }

    var name: String?
    /// In file order.
    var scripts: [Script]
    /// `workspaces` as an array, or `workspaces.packages`.
    var workspaces: [String]
    /// The `packageManager` field, e.g. `pnpm@9.1.0`.
    var packageManager: String?

    init?(_ text: String) {
        guard let data = text.data(using: .utf8),
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        else { return nil }
        name = json["name"] as? String
        packageManager = json["packageManager"] as? String
        if let list = json["workspaces"] as? [String] {
            workspaces = list
        } else {
            workspaces = (json["workspaces"] as? [String: Any])?["packages"] as? [String] ?? []
        }
        // JSONSerialization loses key order; recover it from the text.
        let scripts = json["scripts"] as? [String: Any] ?? [:]
        let order = Self.scriptOrder(in: text, names: Set(scripts.keys))
        self.scripts = order.compactMap { name in (scripts[name] as? String).map { Script(name: name, body: $0) } }
    }

    /// `@acme/web` → `web`.
    var unscopedName: String? {
        guard let name, !name.isEmpty else { return nil }
        return name.split(separator: "/").last.map(String.init)
    }

    private static func scriptOrder(in text: String, names: Set<String>) -> [String] {
        guard let start = text.range(of: "\"scripts\"") else { return names.sorted() }
        var order: [String] = []
        var rest = text[start.upperBound...]
        while order.count < names.count, let quote = rest.firstIndex(of: "\"") {
            let afterQuote = rest.index(after: quote)
            guard let close = rest[afterQuote...].firstIndex(of: "\"") else { break }
            let word = String(rest[afterQuote..<close])
            rest = rest[rest.index(after: close)...]
            if names.contains(word), !order.contains(word),
               rest.drop(while: \.isWhitespace).first == ":" {
                order.append(word)
            }
        }
        return order + names.subtracting(order).sorted()
    }
}

/// The packages of a Node workspace monorepo, from `pnpm-workspace.yaml` or
/// the root `package.json`'s `workspaces`.
struct NodeWorkspaces {
    var patterns: [String]
    /// Folders with a `package.json` matching the patterns, relative to the root.
    var packagePaths: [String]

    init(root: ScannedDirectory) {
        var patterns: [String] = []
        if let text = root.read("pnpm-workspace.yaml") {
            let yaml = YAMLLines(text)
            if let line = yaml.lines.first(where: { $0.indent == 0 && $0.keyValue?.key == "packages" }) {
                patterns = YAMLLines.sequence(value: line.keyValue!.value, following: yaml.block(underTopLevelKey: "packages"))
            }
        } else if let manifest = root.read("package.json").flatMap(PackageManifest.init) {
            patterns = manifest.workspaces
        }
        self.patterns = patterns
        packagePaths = Array(Set(root.expandingGlobs(patterns)))
            .filter { root.contains("\($0)/package.json") }
            .sorted()
    }
}

/// The package manager a folder's scripts run with.
enum PackageManager: String, Equatable {
    case npm, yarn, pnpm, bun

    /// The `packageManager` field of the nearest `package.json` that has one
    /// wins; otherwise the nearest lockfile, walking up to the Project root;
    /// npm when there is none.
    static func detect(for directory: ScannedDirectory) -> PackageManager {
        let lockfiles: [(String, PackageManager)] = [
            ("pnpm-lock.yaml", .pnpm), ("yarn.lock", .yarn), ("bun.lock", .bun), ("bun.lockb", .bun),
            ("package-lock.json", .npm), ("npm-shrinkwrap.json", .npm),
        ]
        for folder in directory.selfAndAncestors {
            if let field = folder.read("package.json").flatMap(PackageManifest.init)?.packageManager,
               let name = field.split(separator: "@").first,
               let manager = PackageManager(rawValue: String(name)) {
                return manager
            }
            if let (_, manager) = lockfiles.first(where: { folder.contains($0.0) }) { return manager }
        }
        return .npm
    }

    func run(_ script: String) -> String {
        "\(rawValue) run \(shellQuoted(script))"
    }

    /// What separates the script from extra arguments: npm needs `--`.
    var forwardingArguments: String {
        self == .npm ? " --" : ""
    }
}

/// A dev server recognised from a script's command.
struct DevServer: Equatable {
    var port: Int?
    /// The flag that sets its port, when the tool doesn't read `PORT` itself
    /// and the script doesn't already pick a port.
    var portFlag: String?
    /// Whether a Run can use another port: false when the script fixes a literal port.
    var canMove: Bool

    /// Tools whose dev server port is known: subcommands (`nil` = none) → default port.
    private struct Tool {
        var names: Set<String>
        var subcommands: [String?: Int]
        /// `nil` when the tool reads `PORT` from the environment.
        var portFlag: String?
    }

    private static let tools: [Tool] = [
        Tool(names: ["next"], subcommands: ["dev": 3000, "start": 3000], portFlag: nil),
        Tool(names: ["nuxt", "nuxi"], subcommands: ["dev": 3000, "preview": 3000], portFlag: nil),
        Tool(names: ["react-scripts"], subcommands: ["start": 3000], portFlag: nil),
        Tool(names: ["vite"], subcommands: [nil: 5173, "dev": 5173, "serve": 5173, "preview": 4173], portFlag: "--port"),
        Tool(names: ["astro"], subcommands: ["dev": 4321, "start": 4321, "preview": 4321], portFlag: "--port"),
        Tool(names: ["ng"], subcommands: ["serve": 4200, "s": 4200], portFlag: "--port"),
        Tool(names: ["storybook"], subcommands: ["dev": 6006], portFlag: "-p"),
        Tool(names: ["start-storybook"], subcommands: [nil: 6006], portFlag: "-p"),
        Tool(names: ["wrangler"], subcommands: ["dev": 8787], portFlag: "--port"),
        Tool(names: ["gatsby"], subcommands: ["develop": 8000, "serve": 9000], portFlag: "-p"),
        Tool(names: ["webpack"], subcommands: ["serve": 8080], portFlag: "--port"),
        Tool(names: ["webpack-dev-server"], subcommands: [nil: 8080], portFlag: "--port"),
        Tool(names: ["docusaurus"], subcommands: ["start": 3000], portFlag: "--port"),
        Tool(names: ["expo"], subcommands: ["start": 8081], portFlag: "--port"),
    ]

    /// Wrappers that run the command after them.
    private static let wrappers: Set = ["cross-env", "npx", "pnpx", "bunx", "dotenv", "env"]

    /// The dev server a script starts, if it is a single command of a known tool.
    static func recognize(_ script: String) -> DevServer? {
        guard !["&&", "||", ";", "|", "&"].contains(where: script.contains) else { return nil }
        var words = script.split(whereSeparator: \.isWhitespace).map(String.init)[...]
        while let first = words.first, first.contains("=") || wrappers.contains(first) || first == "--" {
            words = words.dropFirst()
        }
        guard let tool = tools.first(where: { words.first.map($0.names.contains) == true }) else { return nil }
        let arguments = Array(words.dropFirst())
        let subcommand = arguments.first.flatMap { $0.hasPrefix("-") ? nil : $0 }
        guard let defaultPort = tool.subcommands[subcommand] else { return nil }

        switch explicitPort(in: arguments) {
        case .literal(let port): return DevServer(port: port, portFlag: nil, canMove: false)
        case .variable: return DevServer(port: defaultPort, portFlag: nil, canMove: true)
        case .none: return DevServer(port: defaultPort, portFlag: tool.portFlag, canMove: true)
        }
    }

    private enum ExplicitPort { case literal(Int), variable }

    /// A port the script already passes: `--port 3001`, `-p=3001`, `--port $PORT`.
    private static func explicitPort(in arguments: [String]) -> ExplicitPort? {
        for (index, argument) in arguments.enumerated() {
            var value: String?
            for flag in ["--port", "-p"] {
                if argument == flag, index + 1 < arguments.count { value = arguments[index + 1] }
                if argument.hasPrefix(flag + "=") { value = String(argument.dropFirst(flag.count + 1)) }
            }
            guard let value else { continue }
            if let port = Int(value) { return .literal(port) }
            if value.contains("PORT") { return .variable }
        }
        return nil
    }
}
