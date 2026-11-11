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

    /// The virtual environment a new Service of the Project at `projectPath`
    /// would run in (`.venv`, `../virtualenv`), when its command starts a
    /// Python program; `nil` otherwise.
    public func pythonEnvironment(
        projectPath: String,
        workingDirectory: String,
        commandLine: String
    ) async -> String? {
        await detectPythonEnvironment(
            projectPath: projectPath,
            workingDirectory: workingDirectory,
            commandLine: commandLine
        )
    }
}

extension ServiceSuggestion {
    /// Several reviewed Suggestions as one: Compose services of one folder
    /// run together as `Infra`, anything else keeps the first one's command
    /// line to edit. `nil` for none.
    public static func merged(_ suggestions: [ServiceSuggestion]) -> ServiceSuggestion? {
        mergeServiceSuggestions(suggestions: suggestions)
    }
}
