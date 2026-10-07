import Foundation

/// The short, human-readable, stable handle of a Project or Service
/// (e.g. `my-saas`, `api`).
///
/// A Key is a slug: lowercase ASCII letters and digits in groups separated by
/// single hyphens, at most 64 characters. A `Key` value is always valid.
public struct Key: Hashable, Sendable, Codable, CustomStringConvertible {
    public static let maxLength = 64

    public let rawValue: String

    public init(_ rawValue: String) throws(LocalhostError) {
        guard Key.isValid(rawValue) else { throw .invalidKey(rawValue) }
        self.rawValue = rawValue
    }

    /// A Key derived from a display name, e.g. `"My SaaS"` → `my-saas`.
    /// Returns `nil` when the name has no usable characters.
    public init?(suggestedFrom name: String) {
        let folded = name.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
        var slug = ""
        for scalar in folded.unicodeScalars {
            if ("a"..."z").contains(scalar) || ("0"..."9").contains(scalar) {
                slug.unicodeScalars.append(scalar)
            } else if !slug.isEmpty, slug.last != "-" {
                slug.append("-")
            }
        }
        slug = String(slug.prefix(Key.maxLength))
        while slug.hasSuffix("-") { slug.removeLast() }
        guard let key = try? Key(slug) else { return nil }
        self = key
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

    private static func isValid(_ raw: String) -> Bool {
        guard !raw.isEmpty, raw.count <= maxLength else { return false }
        return raw.split(separator: "-", omittingEmptySubsequences: false).allSatisfy { group in
            !group.isEmpty && group.unicodeScalars.allSatisfy {
                ("a"..."z").contains($0) || ("0"..."9").contains($0)
            }
        }
    }
}
