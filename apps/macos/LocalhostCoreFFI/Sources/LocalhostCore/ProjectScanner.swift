import Foundation

/// Suggests Services from what a Project folder contains: Compose files,
/// package.json scripts, Python and Cargo projects.
public struct ProjectScanner: Sendable {
    public init() {}

    /// Suggestions for a folder being added as a Project.
    public func scan(projectAt path: String) async -> [ServiceSuggestion] {
        await scanFolder(path: path)
    }

    /// Suggestions for an existing Project, without what its Services cover.
    public func scan(_ project: Project) async -> [ServiceSuggestion] {
        await scanProject(project: project)
    }
}
