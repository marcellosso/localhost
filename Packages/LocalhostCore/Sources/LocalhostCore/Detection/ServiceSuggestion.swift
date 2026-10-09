import Foundation

/// A Service that scanning a Project's folder suggests. It is only a
/// proposal: the user reviews and edits it before anything is saved, and an
/// unchecked suggestion is never added.
///
/// Suggestions are plain command lines like any Service. Detectors only shape
/// them, so a suggested Service is never inherently "a Node service".
public struct ServiceSuggestion: Hashable, Sendable, Codable {
    public var name: String
    public var key: Key
    /// Relative to the Project's path; `.` is the Project root.
    public var workingDirectory: String
    public var commandLine: String
    public var port: Int?
    /// Off when the command line can't take another port (e.g. a port fixed
    /// in a Compose file or written into the script).
    public var portCanChange: Bool
    /// Whether the review step checks it by default.
    public var isRecommended: Bool
    /// The file the suggestion comes from, relative to the Project's path,
    /// e.g. `apps/web/package.json`.
    public var foundIn: String
    /// What in that file it comes from, e.g. `"dev": "vite"`.
    public var detail: String

    public init(
        name: String,
        key: Key,
        workingDirectory: String,
        commandLine: String,
        port: Int? = nil,
        portCanChange: Bool = true,
        isRecommended: Bool,
        foundIn: String,
        detail: String
    ) {
        self.name = name
        self.key = key
        self.workingDirectory = workingDirectory
        self.commandLine = commandLine
        self.port = port
        self.portCanChange = portCanChange
        self.isRecommended = isRecommended
        self.foundIn = foundIn
        self.detail = detail
    }
}

extension Key {
    /// A Key for a suggestion's name, `service` when the name has no usable characters.
    static func suggestion(_ name: String) -> Key {
        Key(suggestedFrom: name) ?? (try! Key("service"))
    }
}
