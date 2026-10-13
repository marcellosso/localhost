import Foundation

// A Project file (YAML) read or about to be written. The core holds the
// document; this keeps the Swift core's value-type API over it.

extension ProjectDocument {
    public typealias Service = DocumentService
    public typealias Profile = DocumentProfile

    /// The document exporting `project`.
    public convenience init(_ project: Project) {
        self.init(unsafeFromHandle: ProjectDocument.ffiFromProject(project: project).uniffiCloneHandle())
    }

    /// Reads a Project file.
    public convenience init(yaml text: String) throws(LocalhostError) {
        let document = try core { try ProjectDocument.ffiFromYaml(text: text) }
        self.init(unsafeFromHandle: document.uniffiCloneHandle())
    }

    public var name: String { ffiName() }
    public var key: Key { ffiKey() }
    /// The Project folder.
    public var path: String { ffiPath() }
    public var variables: Variables { ffiVariables() }
    public var services: [Service] { ffiServices() }
    public var profiles: [Profile] { ffiProfiles() }
    public var activeProfile: String? { ffiActiveProfile() }
    public var yaml: String { ffiToYaml() }

    /// A copy whose Project lives at `path`.
    public func withPath(_ path: String) -> ProjectDocument {
        ffiWithPath(path: path)
    }
}

// A document is a value the app passes around (e.g. in a sheet); two are the
// same if they are the same object.
extension ProjectDocument: Hashable {
    public static func == (lhs: ProjectDocument, rhs: ProjectDocument) -> Bool { lhs === rhs }
    public func hash(into hasher: inout Hasher) { hasher.combine(ObjectIdentifier(self)) }
}
