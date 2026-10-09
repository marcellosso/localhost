import Foundation

/// Suggests one Service per Compose service, run in the foreground
/// (`docker compose up <service>`) so Stop's SIGINT shuts it down cleanly.
/// Its Port is the first published host port; it can't move, since the
/// port is fixed in the Compose file.
public struct ComposeDetector: ServiceDetector {
    /// In the order Docker Compose picks them; only the first one present is used.
    static let fileNames = ["compose.yaml", "compose.yml", "docker-compose.yaml", "docker-compose.yml"]

    public init() {}

    public func suggestions(in directory: ScannedDirectory) -> [ServiceSuggestion] {
        guard let fileName = Self.fileNames.first(where: directory.contains),
              let text = directory.read(fileName)
        else { return [] }
        return Self.services(in: text).map { service in
            ServiceSuggestion(
                name: service.name,
                key: .suggestion(service.name),
                workingDirectory: directory.path,
                commandLine: "docker compose up \(shellQuoted(service.name))",
                port: service.port,
                portCanChange: false,
                isRecommended: true,
                foundIn: directory.relativePath(fileName),
                detail: service.image.map { "Compose service · \($0)" } ?? "Compose service"
            )
        }
    }

    struct ComposeService: Equatable {
        var name: String
        var image: String?
        /// The first published host port.
        var port: Int?
    }

    /// The Compose services of a Compose file, in file order.
    static func services(in text: String) -> [ComposeService] {
        let block = YAMLLines(text).block(underTopLevelKey: "services")
        guard let serviceIndent = block.first?.indent else { return [] }
        var services: [ComposeService] = []
        var index = block.startIndex
        while index < block.endIndex {
            let line = block[index]
            index += 1
            guard line.indent == serviceIndent, !line.isListItem, let (name, _) = line.keyValue else { continue }
            let body = block[index...].prefix { $0.indent > serviceIndent }
            index += body.count
            services.append(service(named: name, body: body))
        }
        return services
    }

    private static func service(named name: String, body: ArraySlice<YAMLLines.Line>) -> ComposeService {
        var service = ComposeService(name: name)
        guard let propertyIndent = body.first?.indent else { return service }
        for (offset, line) in zip(body.indices, body) where line.indent == propertyIndent && !line.isListItem {
            guard let (key, value) = line.keyValue else { continue }
            let following = body[(offset + 1)...].prefix {
                $0.indent > propertyIndent || ($0.indent == propertyIndent && $0.isListItem)
            }
            switch key {
            case "image":
                service.image = YAMLLines.unquoted(value)
            case "ports":
                service.port = firstPublishedPort(value: value, following: following)
            default:
                break
            }
        }
        return service
    }

    private static func firstPublishedPort(value: String, following: ArraySlice<YAMLLines.Line>) -> Int? {
        if value.hasPrefix("[") || following.first.map({ $0.keyValue == nil }) == true {
            return YAMLLines.sequence(value: value, following: following).lazy.compactMap { publishedPort(shortSyntax: $0) }.first
        }
        // Long syntax: `- target: 80` / `published: "8080"` mappings.
        return following.lazy
            .compactMap { $0.keyValue }
            .first { $0.key == "published" }
            .flatMap { publishedPort(fromHostPart: YAMLLines.unquoted($0.value)) }
    }

    /// The host port of a short-syntax entry such as `5432:5432`,
    /// `127.0.0.1:8080:80/tcp` or `${PORT:-3000}:3000`; `nil` when only a
    /// container port is given (Docker picks the host port).
    static func publishedPort(shortSyntax entry: String) -> Int? {
        let withoutProtocol = substitutingDefaults(entry).split(separator: "/").first.map(String.init) ?? ""
        let parts = withoutProtocol.split(separator: ":", omittingEmptySubsequences: false)
        switch parts.count {
        case 2: return publishedPort(fromHostPart: String(parts[0]))
        case 3: return publishedPort(fromHostPart: String(parts[1]))
        default: return nil
        }
    }

    private static func publishedPort(fromHostPart part: String) -> Int? {
        let first = substitutingDefaults(part).split(separator: "-").first.map(String.init) ?? ""
        guard let port = Int(first), (1...65_535).contains(port) else { return nil }
        return port
    }

    /// Replaces `${VAR:-default}` / `${VAR-default}` with the default and
    /// `${VAR}` with nothing.
    private static func substitutingDefaults(_ text: String) -> String {
        var result = ""
        var rest = Substring(text)
        while let start = rest.range(of: "${") {
            result += rest[..<start.lowerBound]
            guard let end = rest[start.upperBound...].firstIndex(of: "}") else { return result + rest[start.lowerBound...] }
            let expression = rest[start.upperBound..<end]
            if let dash = expression.firstIndex(of: "-") {
                result += expression[expression.index(after: dash)...]
            }
            rest = rest[rest.index(after: end)...]
        }
        return result + rest
    }
}
