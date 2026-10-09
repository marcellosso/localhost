import Foundation

/// Suggests Services for a Python project (a folder with `pyproject.toml`,
/// `requirements.txt`, `setup.py`, `Pipfile` or `manage.py`): a Django
/// `runserver`, a FastAPI app under uvicorn, a Flask app, and the scripts
/// `pyproject.toml` declares. Commands run through the project's tool
/// (uv, Poetry, Pipenv), its `.venv`, or `python3`.
public struct PythonDetector: ServiceDetector {
    static let markers = ["pyproject.toml", "requirements.txt", "setup.py", "Pipfile", "manage.py"]

    public init() {}

    public func suggestions(in directory: ScannedDirectory) -> [ServiceSuggestion] {
        guard Self.markers.contains(where: directory.contains) else { return [] }
        let pyproject = directory.read("pyproject.toml").map(TOMLDocument.init)
        let runner = PythonRunner.detect(for: directory, pyproject: pyproject)
        let baseName = pyproject.flatMap { Self.projectName(in: $0) } ?? directory.name
        var suggestions: [ServiceSuggestion] = []

        func suggest(_ name: String, _ commandLine: String, port: Int? = nil, isRecommended: Bool, foundIn: String, detail: String) {
            suggestions.append(ServiceSuggestion(
                name: name, key: .suggestion(name), workingDirectory: directory.path, commandLine: commandLine,
                port: port, isRecommended: isRecommended, foundIn: directory.relativePath(foundIn), detail: detail
            ))
        }

        if directory.contains("manage.py") {
            suggest(baseName, "\(runner.python) manage.py runserver $PORT", port: 8000, isRecommended: true,
                    foundIn: "manage.py", detail: "Django")
        } else if let app = Self.findApp(in: directory) {
            switch app.framework {
            case .fastAPI:
                let appDirectory = app.isInSourceFolder ? " --app-dir src" : ""
                suggest(baseName, "\(runner.tool("uvicorn")) \(app.module):\(app.variable) --reload\(appDirectory) --port $PORT",
                        port: 8000, isRecommended: true, foundIn: app.file, detail: "FastAPI app `\(app.variable)`")
            case .flask:
                suggest(baseName, "\(runner.tool("flask")) --app \(app.module):\(app.variable) run --debug --port $PORT",
                        port: 5000, isRecommended: true, foundIn: app.file, detail: "Flask app `\(app.variable)`")
            }
        }

        if runner != .system, let pyproject {
            let scripts = (pyproject.table("project.scripts")?.values ?? [:])
                .merging(pyproject.table("tool.poetry.scripts")?.values ?? [:]) { first, _ in first }
            for name in scripts.keys.sorted() {
                suggest("\(baseName) \(name)", runner.run(name), isRecommended: false,
                        foundIn: "pyproject.toml", detail: "script \"\(name)\" = \(scripts[name]!)")
            }
        }
        return suggestions
    }

    private static func projectName(in pyproject: TOMLDocument) -> String? {
        TOMLDocument.string(pyproject.table("project")?.values["name"])
            ?? TOMLDocument.string(pyproject.table("tool.poetry")?.values["name"])
    }

    // MARK: Finding the app

    enum Framework { case fastAPI, flask }

    struct App: Equatable {
        var framework: Framework
        /// Relative to the scanned folder, e.g. `app/main.py`.
        var file: String
        /// Importable module, e.g. `app.main` (without `src.`).
        var module: String
        var variable: String
        var isInSourceFolder: Bool
    }

    static let ignoredFolderNames: Set<String> = [
        "tests", "test", "migrations", "node_modules", "venv", "env", "__pycache__", "site-packages",
        "build", "dist", "docs",
    ]
    static let preferredFileNames = ["main.py", "app.py", "server.py", "api.py", "asgi.py", "wsgi.py"]

    /// The FastAPI or Flask app defined at module level in the folder's
    /// Python files (up to three levels deep), preferring `main.py`/`app.py`
    /// and shallower files.
    static func findApp(in directory: ScannedDirectory) -> App? {
        let files = pythonFiles(in: directory, prefix: "", depthLeft: 3)
            .sorted { lhs, rhs in
                func rank(_ path: String) -> (Int, Int, String) {
                    let name = (path as NSString).lastPathComponent
                    return (path.split(separator: "/").count, preferredFileNames.firstIndex(of: name) ?? preferredFileNames.count, path)
                }
                return rank(lhs) < rank(rhs)
            }
        for file in files.prefix(300) {
            guard let text = directory.read(file), let (framework, variable) = appDefinition(in: text) else { continue }
            let isInSourceFolder = file.hasPrefix("src/")
            var components = (isInSourceFolder ? String(file.dropFirst(4)) : file)
                .dropLast(3).split(separator: "/").map(String.init)
            if components.last == "__init__" { components.removeLast() }
            guard !components.isEmpty else { continue }
            return App(framework: framework, file: file, module: components.joined(separator: "."),
                       variable: variable, isInSourceFolder: isInSourceFolder)
        }
        return nil
    }

    private static func pythonFiles(in directory: ScannedDirectory, prefix: String, depthLeft: Int) -> [String] {
        let files = directory.fileNames().filter { $0.hasSuffix(".py") }.map { prefix + $0 }
        guard depthLeft > 1 else { return files }
        return files + directory.subdirectoryNames()
            .filter { !$0.hasPrefix(".") && !ignoredFolderNames.contains($0) }
            .flatMap { pythonFiles(in: directory.subdirectory($0), prefix: prefix + $0 + "/", depthLeft: depthLeft - 1) }
    }

    /// `app = FastAPI(` / `app: FastAPI = fastapi.FastAPI(` / `app = Flask(__name__)` at the start of a line.
    static func appDefinition(in text: String) -> (Framework, String)? {
        let pattern = /^([A-Za-z_][A-Za-z0-9_]*)\s*(?::\s*[\w.]+\s*)?=\s*(?:[\w]+\.)?(FastAPI|Flask)\(/.anchorsMatchLineEndings()
        guard let match = text.firstMatch(of: pattern) else { return nil }
        return (match.output.2 == "FastAPI" ? .fastAPI : .flask, String(match.output.1))
    }
}

/// How a Python project's commands are run.
enum PythonRunner: Equatable {
    case uv, poetry, pipenv
    /// A virtual environment folder inside the project, e.g. `.venv`.
    case virtualEnvironment(String)
    /// `python3` from the login shell's `PATH`.
    case system

    /// uv or Poetry from a lockfile (walking up to the Project root, for
    /// workspaces) or `pyproject.toml`; Pipenv from a `Pipfile`; else a
    /// `.venv`/`venv` with a Python in it; else the system Python.
    static func detect(for directory: ScannedDirectory, pyproject: TOMLDocument?) -> PythonRunner {
        for folder in directory.selfAndAncestors {
            if folder.contains("uv.lock") { return .uv }
            if folder.contains("poetry.lock") { return .poetry }
            if folder.contains("Pipfile.lock") || folder.contains("Pipfile") { return .pipenv }
        }
        if pyproject?.table("tool.poetry") != nil { return .poetry }
        if pyproject?.tables.contains(where: { $0.name == "tool.uv" || $0.name.hasPrefix("tool.uv.") }) == true { return .uv }
        if let folder = [".venv", "venv"].first(where: { directory.contains("\($0)/bin/python") }) {
            return .virtualEnvironment(folder)
        }
        return .system
    }

    var python: String {
        switch self {
        case .uv, .poetry, .pipenv: run("python")
        case .virtualEnvironment(let folder): "\(folder)/bin/python"
        case .system: "python3"
        }
    }

    /// A command-line tool installed in the project's environment.
    func tool(_ name: String) -> String {
        switch self {
        case .uv, .poetry, .pipenv: run(name)
        case .virtualEnvironment(let folder): "\(folder)/bin/\(name)"
        case .system: "python3 -m \(name)"
        }
    }

    func run(_ command: String) -> String {
        switch self {
        case .uv: "uv run \(command)"
        case .poetry: "poetry run \(command)"
        case .pipenv: "pipenv run \(command)"
        case .virtualEnvironment(let folder): "\(folder)/bin/\(command)"
        case .system: command
        }
    }
}
