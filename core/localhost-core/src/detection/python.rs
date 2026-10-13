use std::collections::BTreeMap;

use super::manifest::TomlDocument;
use super::{ScannedDirectory, ServiceDetector, ServiceSuggestion};

/// Suggests Services for a Python project (a folder with `pyproject.toml`,
/// `requirements.txt`, `setup.py`, `Pipfile` or `manage.py`): a Django
/// `runserver`, a FastAPI app under uvicorn, a Flask app, and the scripts
/// `pyproject.toml` declares. Commands run through the project's tool (uv,
/// Poetry, Pipenv), its `.venv`, or `python3`.
pub struct PythonDetector;

const MARKERS: [&str; 5] = ["pyproject.toml", "requirements.txt", "setup.py", "Pipfile", "manage.py"];

impl ServiceDetector for PythonDetector {
    fn suggestions(&self, directory: &ScannedDirectory) -> Vec<ServiceSuggestion> {
        if !MARKERS.iter().any(|marker| directory.contains(marker)) {
            return Vec::new();
        }
        let pyproject = directory.read("pyproject.toml").map(|text| TomlDocument::parse(&text));
        let runner = PythonRunner::detect(directory, pyproject.as_ref());
        let base_name = pyproject.as_ref().and_then(project_name).unwrap_or_else(|| directory.name());
        let mut suggestions = Vec::new();
        let mut suggest = |name: String,
                           command_line: String,
                           port: Option<u16>,
                           recommended: bool,
                           found_in: &str,
                           detail: String| {
            let mut suggestion = ServiceSuggestion::new(
                name,
                &directory.path,
                command_line,
                recommended,
                directory.relative_path(found_in),
                detail,
            );
            suggestion.port = port;
            suggestions.push(suggestion);
        };

        if directory.contains("manage.py") {
            let command_line = format!("{} manage.py runserver $PORT", runner.python());
            suggest(base_name.clone(), command_line, Some(8000), true, "manage.py", "Django".into());
        } else if let Some(app) = find_app(directory) {
            match app.framework {
                Framework::FastApi => {
                    let app_directory = if app.is_in_source_folder { " --app-dir src" } else { "" };
                    let command_line = format!(
                        "{} {}:{} --reload{app_directory} --port $PORT",
                        runner.tool("uvicorn"),
                        app.module,
                        app.variable
                    );
                    let detail = format!("FastAPI app `{}`", app.variable);
                    suggest(base_name.clone(), command_line, Some(8000), true, &app.file, detail);
                }
                Framework::Flask => {
                    let command_line = format!(
                        "{} --app {}:{} run --debug --port $PORT",
                        runner.tool("flask"),
                        app.module,
                        app.variable
                    );
                    let detail = format!("Flask app `{}`", app.variable);
                    suggest(base_name.clone(), command_line, Some(5000), true, &app.file, detail);
                }
            }
        }

        if runner != PythonRunner::System
            && let Some(pyproject) = &pyproject
        {
            let mut scripts: BTreeMap<&String, &String> = BTreeMap::new();
            for table in ["tool.poetry.scripts", "project.scripts"] {
                // `project.scripts` wins over Poetry's.
                if let Some(table) = pyproject.table(table) {
                    scripts.extend(table.values.iter());
                }
            }
            for (name, value) in scripts {
                let detail = format!("script \"{name}\" = {value}");
                suggest(format!("{base_name} {name}"), runner.run(name), None, false, "pyproject.toml", detail);
            }
        }
        suggestions
    }
}

fn project_name(pyproject: &TomlDocument) -> Option<String> {
    TomlDocument::string(pyproject.table("project").and_then(|table| table.values.get("name")))
        .or_else(|| TomlDocument::string(pyproject.table("tool.poetry").and_then(|table| table.values.get("name"))))
}

// MARK: Finding the app

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framework {
    FastApi,
    Flask,
}

#[derive(Debug, PartialEq, Eq)]
struct App {
    framework: Framework,
    /// Relative to the scanned folder, e.g. `app/main.py`.
    file: String,
    /// Importable module, e.g. `app.main` (without `src.`).
    module: String,
    variable: String,
    is_in_source_folder: bool,
}

const IGNORED_FOLDER_NAMES: [&str; 11] = [
    "tests",
    "test",
    "migrations",
    "node_modules",
    "venv",
    "env",
    "__pycache__",
    "site-packages",
    "build",
    "dist",
    "docs",
];
const PREFERRED_FILE_NAMES: [&str; 6] = ["main.py", "app.py", "server.py", "api.py", "asgi.py", "wsgi.py"];

/// The FastAPI or Flask app defined at module level in the folder's Python
/// files (up to three levels deep), preferring `main.py`/`app.py` and
/// shallower files.
fn find_app(directory: &ScannedDirectory) -> Option<App> {
    let mut files = python_files(directory, "", 3);
    let rank = |path: &String| {
        let name = path.rsplit('/').next().unwrap_or_default();
        let preference =
            PREFERRED_FILE_NAMES.iter().position(|preferred| *preferred == name).unwrap_or(PREFERRED_FILE_NAMES.len());
        (path.split('/').count(), preference, path.clone())
    };
    files.sort_by_key(rank);
    for file in files.iter().take(300) {
        let Some(text) = directory.read(file) else { continue };
        let Some((framework, variable)) = app_definition(&text) else { continue };
        let is_in_source_folder = file.starts_with("src/");
        let relative = if is_in_source_folder { &file[4..] } else { file.as_str() };
        let mut components: Vec<&str> = relative[..relative.len() - 3].split('/').filter(|c| !c.is_empty()).collect();
        if components.last() == Some(&"__init__") {
            components.pop();
        }
        if components.is_empty() {
            continue;
        }
        return Some(App {
            framework,
            file: file.clone(),
            module: components.join("."),
            variable,
            is_in_source_folder,
        });
    }
    None
}

fn python_files(directory: &ScannedDirectory, prefix: &str, depth_left: usize) -> Vec<String> {
    let mut files: Vec<String> = directory
        .file_names()
        .into_iter()
        .filter(|name| name.ends_with(".py"))
        .map(|name| format!("{prefix}{name}"))
        .collect();
    if depth_left > 1 {
        for name in directory.subdirectory_names() {
            if name.starts_with('.') || IGNORED_FOLDER_NAMES.contains(&name.as_str()) {
                continue;
            }
            files.extend(python_files(&directory.subdirectory(&name), &format!("{prefix}{name}/"), depth_left - 1));
        }
    }
    files
}

/// `app = FastAPI(` / `app: FastAPI = fastapi.FastAPI(` / `app = Flask(__name__)`
/// at the start of a line. Whitespace between the parts may span lines.
fn app_definition(text: &str) -> Option<(Framework, String)> {
    let line_starts = std::iter::once(0).chain(text.match_indices('\n').map(|(index, _)| index + 1));
    line_starts.filter_map(|start| app_definition_at(&text[start..])).next()
}

fn app_definition_at(text: &str) -> Option<(Framework, String)> {
    let is_word = |c: char| c == '_' || c.is_alphanumeric();
    let mut rest = text;
    // The variable: [A-Za-z_][A-Za-z0-9_]*
    let first = rest.chars().next()?;
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return None;
    }
    let variable_length = rest.find(|c: char| !(c == '_' || c.is_ascii_alphanumeric())).unwrap_or(rest.len());
    let variable = &rest[..variable_length];
    rest = rest[variable_length..].trim_start();
    // An optional annotation: `: some.Type`
    if let Some(annotation) = rest.strip_prefix(':') {
        let annotation = annotation.trim_start();
        let type_length = annotation.find(|c: char| !(is_word(c) || c == '.')).unwrap_or(annotation.len());
        if type_length == 0 {
            return None;
        }
        rest = annotation[type_length..].trim_start();
    }
    rest = rest.strip_prefix('=')?.trim_start();
    // An optional module prefix: `fastapi.`
    let word_length = rest.find(|c: char| !is_word(c)).unwrap_or(rest.len());
    if word_length > 0 && rest[word_length..].starts_with('.') {
        let after_prefix = &rest[word_length + 1..];
        if !(after_prefix.starts_with("FastAPI(") || after_prefix.starts_with("Flask(")) {
            return None;
        }
        rest = after_prefix;
    }
    if rest.starts_with("FastAPI(") {
        Some((Framework::FastApi, variable.to_string()))
    } else if rest.starts_with("Flask(") {
        Some((Framework::Flask, variable.to_string()))
    } else {
        None
    }
}

/// How a Python project's commands are run.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PythonRunner {
    Uv,
    Poetry,
    Pipenv,
    /// A virtual environment folder inside the project, e.g. `.venv`.
    VirtualEnvironment(&'static str),
    /// `python3` from the login shell's `PATH`.
    System,
}

impl PythonRunner {
    /// uv or Poetry from a lockfile (walking up to the Project root, for
    /// workspaces) or `pyproject.toml`; Pipenv from a `Pipfile`; else a
    /// `.venv`/`venv` with a Python in it; else the system Python.
    fn detect(directory: &ScannedDirectory, pyproject: Option<&TomlDocument>) -> PythonRunner {
        for folder in directory.self_and_ancestors() {
            if folder.contains("uv.lock") {
                return PythonRunner::Uv;
            }
            if folder.contains("poetry.lock") {
                return PythonRunner::Poetry;
            }
            if folder.contains("Pipfile.lock") || folder.contains("Pipfile") {
                return PythonRunner::Pipenv;
            }
        }
        if pyproject.is_some_and(|pyproject| pyproject.table("tool.poetry").is_some()) {
            return PythonRunner::Poetry;
        }
        if pyproject.is_some_and(|p| p.tables.iter().any(|t| t.name == "tool.uv" || t.name.starts_with("tool.uv."))) {
            return PythonRunner::Uv;
        }
        if let Some(folder) =
            [".venv", "venv"].into_iter().find(|folder| directory.contains(&format!("{folder}/bin/python")))
        {
            return PythonRunner::VirtualEnvironment(folder);
        }
        PythonRunner::System
    }

    fn python(&self) -> String {
        match self {
            PythonRunner::Uv | PythonRunner::Poetry | PythonRunner::Pipenv => self.run("python"),
            PythonRunner::VirtualEnvironment(folder) => format!("{folder}/bin/python"),
            PythonRunner::System => "python3".into(),
        }
    }

    /// A command-line tool installed in the project's environment.
    fn tool(&self, name: &str) -> String {
        match self {
            PythonRunner::Uv | PythonRunner::Poetry | PythonRunner::Pipenv => self.run(name),
            PythonRunner::VirtualEnvironment(folder) => format!("{folder}/bin/{name}"),
            PythonRunner::System => format!("python3 -m {name}"),
        }
    }

    fn run(&self, command: &str) -> String {
        match self {
            PythonRunner::Uv => format!("uv run {command}"),
            PythonRunner::Poetry => format!("poetry run {command}"),
            PythonRunner::Pipenv => format!("pipenv run {command}"),
            PythonRunner::VirtualEnvironment(folder) => format!("{folder}/bin/{command}"),
            PythonRunner::System => command.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_app_definitions_at_the_start_of_a_line() {
        let found = app_definition;
        assert_eq!(
            found("from fastapi import FastAPI\n\napp = FastAPI(title=\"API\")\n"),
            Some((Framework::FastApi, "app".into()))
        );
        assert_eq!(found("api: fastapi.FastAPI = fastapi.FastAPI()\n"), Some((Framework::FastApi, "api".into())));
        assert_eq!(found("server=Flask(__name__)"), Some((Framework::Flask, "server".into())));
        assert_eq!(found("    app = FastAPI()\n"), None, "indented: not module level");
        assert_eq!(found("app = make_app(FastAPI())\n"), None);
        assert_eq!(found("app = a.b.FastAPI()\n"), None, "one module prefix at most");
    }
}
