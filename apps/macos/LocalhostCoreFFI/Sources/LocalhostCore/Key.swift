import Foundation

/// A short, stable identifier for a Project or Service: lowercase letters,
/// digits and single hyphens (`my-saas`). Validated by the core.
public struct Key: Hashable, Sendable, Codable, Comparable, CustomStringConvertible {
    public static let maxLength = 64

    public let rawValue: String

    public init(_ rawValue: String) throws(LocalhostError) {
        self = try core { try keyNew(raw: rawValue) }
    }

    /// A Key made from a name (`"My SaaS"` → `my-saas`), `nil` if the name has
    /// no usable characters.
    public init?(suggestedFrom name: String) {
        guard let key = keySuggestedFrom(name: name) else { return nil }
        self = key
    }

    /// A Key the core already validated.
    init(validatedRawValue: String) {
        rawValue = validatedRawValue
    }

    public init(from decoder: any Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        do {
            try self.init(raw)
        } catch {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "Invalid key \"\(raw)\"")
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }

    public var description: String { rawValue }

    public static func < (lhs: Key, rhs: Key) -> Bool { lhs.rawValue < rhs.rawValue }
}
