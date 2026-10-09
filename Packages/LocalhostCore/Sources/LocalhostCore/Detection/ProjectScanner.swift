import Foundation

/// Recognises one kind of project file and suggests Services for it.
///
/// Detectors are small and independent: each looks at one folder at a time
/// and returns plain command lines. None of them makes a Service
/// runtime-specific.
public protocol ServiceDetector: Sendable {
    /// Suggestions for the files directly inside `directory`.
    func suggestions(in directory: ScannedDirectory) -> [ServiceSuggestion]

    /// Folders (relative to the Project) this detector knows belong to the
    /// Project beyond the ones the scanner finds itself, e.g. workspace
    /// packages declared at the Project root.
    func declaredDirectories(in root: ScannedDirectory) -> [String]
}

extension ServiceDetector {
    public func declaredDirectories(in root: ScannedDirectory) -> [String] { [] }
}

/// Scans a Project's folder for common project files and suggests Services.
///
/// Scanning only reads: it never writes into the folder and makes no
/// network calls. It looks at the root, every folder up to two levels deep
/// (skipping dependency, build and hidden folders) and the folders detectors
/// declare, such as workspace packages.
public struct ProjectScanner: Sendable {
    public static let standardDetectors: [any ServiceDetector] = [
        ComposeDetector(), NodeDetector(), PythonDetector(), CargoDetector(),
    ]

    /// Folders that hold dependencies, build output or tooling, never Services.
    static let ignoredFolderNames: Set<String> = [
        "node_modules", "bower_components", "vendor", "venv", "env", "__pycache__", "site-packages",
        "target", "dist", "build", "out", "coverage", "Pods", "DerivedData",
    ]

    static let maximumDirectoryCount = 400

    let detectors: [any ServiceDetector]

    public init(detectors: [any ServiceDetector] = ProjectScanner.standardDetectors) {
        self.detectors = detectors
    }

    /// Suggestions for the Project folder at `path`, folder by folder (root
    /// first), with Keys made unique. A folder with nothing recognisable
    /// yields none.
    @concurrent
    public func scan(projectAt path: String) async -> [ServiceSuggestion] {
        let rootURL = URL(filePath: (path as NSString).expandingTildeInPath, directoryHint: .isDirectory)
            .standardizedFileURL
        let root = ScannedDirectory(root: rootURL, path: ".")
        var suggestions: [ServiceSuggestion] = []
        for directory in directories(under: root) {
            for detector in detectors {
                suggestions += detector.suggestions(in: directory)
            }
        }
        return Self.makingKeysUnique(suggestions)
    }

    func directories(under root: ScannedDirectory) -> [ScannedDirectory] {
        var paths: Set<String> = []
        var level = [root]
        for _ in 0..<2 {
            level = level.flatMap { directory in
                directory.subdirectoryNames()
                    .filter { !$0.hasPrefix(".") && !Self.ignoredFolderNames.contains($0) }
                    .map { directory.subdirectory($0) }
            }
            paths.formUnion(level.map(\.path))
        }
        for detector in detectors {
            paths.formUnion(detector.declaredDirectories(in: root).compactMap(ScannedDirectory.normalized))
        }
        let sorted = paths.sorted().prefix(Self.maximumDirectoryCount)
        return [root] + sorted.map { ScannedDirectory(root: root.root, path: $0) }.filter(\.exists)
    }

    /// Later suggestions whose Key is already taken get `-2`, `-3`, ….
    static func makingKeysUnique(_ suggestions: [ServiceSuggestion]) -> [ServiceSuggestion] {
        var taken: Set<String> = []
        return suggestions.map { suggestion in
            var suggestion = suggestion
            var candidate = suggestion.key.rawValue
            var number = 2
            while taken.contains(candidate) {
                let suffix = "-\(number)"
                candidate = String(suggestion.key.rawValue.prefix(Key.maxLength - suffix.count)) + suffix
                number += 1
            }
            taken.insert(candidate)
            suggestion.key = (try? Key(candidate)) ?? suggestion.key
            return suggestion
        }
    }
}

/// One folder of a Project being scanned, with read-only access to its files.
public struct ScannedDirectory: Sendable {
    /// The Project's root folder.
    public let root: URL
    /// Relative to the root; `.` is the root itself.
    public let path: String

    /// Files larger than this are treated as unreadable.
    static let maximumFileSize = 1_000_000

    init(root: URL, path: String) {
        self.root = root
        self.path = path
    }

    public var url: URL {
        path == "." ? root : root.appending(path: path, directoryHint: .isDirectory)
    }

    public var isRoot: Bool { path == "." }

    /// The folder's own name; the Project folder's name for the root.
    public var name: String { url.lastPathComponent }

    var exists: Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }

    /// `name` relative to the Project root, e.g. `apps/web/package.json`.
    public func relativePath(_ name: String) -> String {
        path == "." ? name : "\(path)/\(name)"
    }

    public func subdirectory(_ name: String) -> ScannedDirectory {
        ScannedDirectory(root: root, path: relativePath(name))
    }

    /// The folder this one is in, `nil` for the root.
    public var parent: ScannedDirectory? {
        guard !isRoot else { return nil }
        let components = path.split(separator: "/").dropLast()
        return ScannedDirectory(root: root, path: components.isEmpty ? "." : components.joined(separator: "/"))
    }

    /// This folder and every folder above it, up to the root.
    public var selfAndAncestors: [ScannedDirectory] {
        Array(sequence(first: self, next: \.parent))
    }

    public func contains(_ name: String) -> Bool {
        FileManager.default.fileExists(atPath: url.appending(path: name).path)
    }

    /// The text of a file in this folder (`name` may contain `/`), `nil` if
    /// it is missing, too large or not UTF-8.
    public func read(_ name: String) -> String? {
        let fileURL = url.appending(path: name)
        guard let size = try? fileURL.resourceValues(forKeys: [.fileSizeKey]).fileSize,
              size <= Self.maximumFileSize,
              let data = try? Data(contentsOf: fileURL)
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    public func subdirectoryNames() -> [String] {
        let contents = (try? FileManager.default.contentsOfDirectory(
            at: url, includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey]
        )) ?? []
        return contents.filter { item in
            let values = try? item.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            return values?.isDirectory == true && values?.isSymbolicLink != true
        }
        .map(\.lastPathComponent)
        .sorted()
    }

    public func fileNames() -> [String] {
        let contents = (try? FileManager.default.contentsOfDirectory(
            at: url, includingPropertiesForKeys: [.isRegularFileKey]
        )) ?? []
        return contents.filter { (try? $0.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile == true }
            .map(\.lastPathComponent)
            .sorted()
    }

    /// Expands workspace-style globs (`apps/*`, `packages/**`, `tools/cli`,
    /// with `!` excludes) relative to this folder into folder paths relative
    /// to the Project root. `**` reaches at most four levels down.
    public func expandingGlobs(_ patterns: [String]) -> [String] {
        var included: [String] = []
        var excluded: [String] = []
        for raw in patterns {
            let isExclude = raw.hasPrefix("!")
            let pattern = isExclude ? String(raw.dropFirst()) : raw
            let components = pattern.split(separator: "/").map(String.init).filter { $0 != "." && !$0.isEmpty }
            guard !components.contains("..") else { continue }
            if isExclude {
                excluded.append(components.joined(separator: "/"))
            } else {
                included += matching(components[...], depthLeft: 4).map(\.path)
            }
        }
        return included.filter { path in
            !excluded.contains { Self.glob($0, matches: relativeToSelf(path)) }
        }
    }

    private func relativeToSelf(_ projectPath: String) -> String {
        isRoot ? projectPath : String(projectPath.dropFirst(path.count + 1))
    }

    private func matching(_ components: ArraySlice<String>, depthLeft: Int) -> [ScannedDirectory] {
        guard let first = components.first else { return [self] }
        let rest = components.dropFirst()
        let children = { subdirectoryNames()
            .filter { !$0.hasPrefix(".") && !ProjectScanner.ignoredFolderNames.contains($0) } }
        if first == "**" {
            guard depthLeft > 0 else { return matching(rest, depthLeft: 0) }
            return matching(rest, depthLeft: depthLeft)
                + children().flatMap { subdirectory($0).matching(components, depthLeft: depthLeft - 1) }
        }
        if first.contains("*") {
            return children().filter { Self.wildcard(first, matches: $0) }
                .flatMap { subdirectory($0).matching(rest, depthLeft: depthLeft) }
        }
        let next = subdirectory(first)
        return next.exists ? next.matching(rest, depthLeft: depthLeft) : []
    }

    /// Whether a path glob (`**` for any number of folders) matches a relative path.
    static func glob(_ pattern: String, matches path: String) -> Bool {
        func match(_ pattern: ArraySlice<Substring>, _ path: ArraySlice<Substring>) -> Bool {
            guard let first = pattern.first else { return path.isEmpty }
            if first == "**" {
                return match(pattern.dropFirst(), path) || (!path.isEmpty && match(pattern, path.dropFirst()))
            }
            guard let component = path.first, wildcard(String(first), matches: String(component)) else { return false }
            return match(pattern.dropFirst(), path.dropFirst())
        }
        return match(pattern.split(separator: "/")[...], path.split(separator: "/")[...])
    }

    /// Whether a single-component pattern with `*` wildcards matches a name.
    static func wildcard(_ pattern: String, matches name: String) -> Bool {
        let parts = pattern.split(separator: "*", omittingEmptySubsequences: false)
        guard parts.count > 1 else { return pattern == name }
        guard name.hasPrefix(parts.first!), name.dropFirst(parts.first!.count).hasSuffix(parts.last!),
              name.count >= parts.first!.count + parts.last!.count
        else { return false }
        var remaining = name.dropFirst(parts.first!.count).dropLast(parts.last!.count)
        for part in parts.dropFirst().dropLast() where !part.isEmpty {
            guard let range = remaining.range(of: part) else { return false }
            remaining = remaining[range.upperBound...]
        }
        return true
    }

    /// A relative folder path without `.` components, `nil` if it leaves the Project.
    static func normalized(_ path: String) -> String? {
        let components = path.split(separator: "/").filter { $0 != "." }
        guard !components.contains(".."), !path.hasPrefix("/") else { return nil }
        return components.isEmpty ? nil : components.joined(separator: "/")
    }
}
