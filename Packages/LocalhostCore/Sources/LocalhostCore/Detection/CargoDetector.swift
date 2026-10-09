import Foundation

/// Suggests `cargo run` for a Cargo package with a binary, and
/// `cargo run --bin <name>` for each of its other binaries. Library-only
/// packages and virtual workspace roots get none; workspace members are
/// scanned as their own folders.
public struct CargoDetector: ServiceDetector {
    public init() {}

    public func declaredDirectories(in root: ScannedDirectory) -> [String] {
        guard let manifest = root.read("Cargo.toml").map(TOMLDocument.init) else { return [] }
        return root.expandingGlobs(TOMLDocument.strings(manifest.table("workspace")?.values["members"]))
    }

    public func suggestions(in directory: ScannedDirectory) -> [ServiceSuggestion] {
        guard let manifest = directory.read("Cargo.toml").map(TOMLDocument.init),
              let package = manifest.table("package"),
              let packageName = TOMLDocument.string(package.values["name"])
        else { return [] }

        let hasMainBinary = directory.contains("src/main.rs")
        var binaries = manifest.tables(named: "bin").compactMap { TOMLDocument.string($0.values["name"]) }
        binaries += directory.subdirectory("src/bin").fileNames()
            .filter { $0.hasSuffix(".rs") }.map { String($0.dropLast(3)) }
        binaries += directory.subdirectory("src/bin").subdirectoryNames()
            .filter { directory.contains("src/bin/\($0)/main.rs") }
        binaries = binaries.reduce(into: []) { unique, name in
            if name != packageName, !unique.contains(name) { unique.append(name) }
        }

        func suggestion(_ name: String, _ commandLine: String, isRecommended: Bool) -> ServiceSuggestion {
            ServiceSuggestion(
                name: name, key: .suggestion(name), workingDirectory: directory.path, commandLine: commandLine,
                isRecommended: isRecommended, foundIn: directory.relativePath("Cargo.toml"),
                detail: isRecommended && hasMainBinary ? "package \"\(packageName)\"" : "binary \"\(name)\""
            )
        }
        var suggestions: [ServiceSuggestion] = []
        if hasMainBinary {
            suggestions.append(suggestion(packageName, "cargo run", isRecommended: true))
        }
        for (index, binary) in binaries.enumerated() {
            let isOnlyBinary = !hasMainBinary && index == 0
            suggestions.append(suggestion(binary, "cargo run --bin \(shellQuoted(binary))", isRecommended: isOnlyBinary))
        }
        return suggestions
    }
}
