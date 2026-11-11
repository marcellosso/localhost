//! Finding the Python virtual environment a Service's command line would
//! run in, so a new Service or Suggestion can name it as its
//! `python_environment` instead of spelling `.venv/bin/` into the command.
//!
//! Like the rest of detection, this only reads: it shapes a suggestion, and
//! a Service with no `python_environment` runs exactly as written.

use std::path::Path;

use super::{CommandSyntax, ScannedDirectory};
use crate::paths::{expand_tilde, standardize};

/// The folder names a virtual environment goes by. A `.env` file holds
/// variables for dotenv, and an `env` folder is too often something else.
pub const ENVIRONMENT_FOLDER_NAMES: [&str; 3] = [".venv", "venv", "virtualenv"];

/// Programs that run inside a Python environment and that a virtual
/// environment would put on `PATH`. Tools managing their own environment
/// (uv, Poetry, Pipenv, PDM, Hatch, Rye, Conda) are left out: their `run`
/// already picks it.
const PYTHON_PROGRAMS: [&str; 20] = [
    "python",
    "python3",
    "py",
    "pip",
    "pip3",
    "uvicorn",
    "gunicorn",
    "hypercorn",
    "daphne",
    "granian",
    "flask",
    "fastapi",
    "django-admin",
    "celery",
    "dramatiq",
    "rq",
    "streamlit",
    "jupyter",
    "pytest",
    "waitress-serve",
];

/// Whether `command_line` starts a program a virtual environment provides:
/// one of `PYTHON_PROGRAMS` (or a versioned `python3.12`), or a `.py`
/// script, after any leading `NAME=value` assignments. A program given by a
/// path into a virtual environment (`.venv/bin/uvicorn`) already picks its
/// environment, so it doesn't count; nor does anything after `&&`, `;` or a
/// pipe, so `source venv/bin/activate && …` is left as the user wrote it.
pub fn runs_in_python_environment(command_line: &str) -> bool {
    let is_assignment =
        |word: &str| word.split_once('=').is_some_and(|(name, _)| crate::validate::is_valid_variable_name(name));
    let Some(program) = command_line.split_whitespace().find(|word| !is_assignment(word)) else {
        return false;
    };
    let components: Vec<&str> = program.split(['/', '\\']).collect();
    if components.len() > 1 && components.iter().any(|component| ENVIRONMENT_FOLDER_NAMES.contains(component)) {
        return false;
    }
    let name = components.last().copied().unwrap_or_default();
    let name = name.strip_suffix(".exe").unwrap_or(name);
    let is_versioned_python = name.strip_prefix("python").is_some_and(|version| {
        !version.is_empty() && version.chars().all(|character| character.is_ascii_digit() || character == '.')
    });
    PYTHON_PROGRAMS.contains(&name) || is_versioned_python || name.ends_with(".py")
}

/// Whether `directory` holds a virtual environment named `name` usable from
/// `syntax`'s shell: a Python in its programs folder (`bin`, or `Scripts` on
/// Windows), or a `pyvenv.cfg` beside that folder. A Linux environment has
/// no `Scripts`, so it doesn't count on Windows, nor a Windows one elsewhere.
pub(crate) fn is_virtual_environment(directory: &ScannedDirectory, name: &str) -> bool {
    let folder = directory.full_path().join(name);
    if !folder.is_dir() {
        return false;
    }
    let (programs, python) = match directory.syntax {
        CommandSyntax::Posix => ("bin", "python"),
        CommandSyntax::Cmd => ("Scripts", "python.exe"),
    };
    folder.join(programs).join(python).exists()
        || (folder.join("pyvenv.cfg").is_file() && folder.join(programs).is_dir())
}

/// The virtual environment closest to `directory`: in it, else in the
/// nearest folder above it, up to the Project root. The path is relative to
/// `directory`, e.g. `.venv` or `../../virtualenv`.
pub(crate) fn nearest(directory: &ScannedDirectory) -> Option<String> {
    directory.self_and_ancestors().iter().enumerate().find_map(|(levels_up, folder)| {
        let name = ENVIRONMENT_FOLDER_NAMES.into_iter().find(|name| is_virtual_environment(folder, name))?;
        Some(format!("{}{name}", "../".repeat(levels_up)))
    })
}

/// The `python_environment` to give a new Service: the virtual environment
/// nearest its working directory (see `nearest`), when its command line
/// runs in one (see `runs_in_python_environment`). `None` otherwise, or when
/// the working directory leaves the Project, whose folders above it aren't
/// searched. Reads the file system.
pub fn detect_python_environment(
    project_path: &str,
    working_directory: &str,
    command_line: &str,
    syntax: CommandSyntax,
) -> Option<String> {
    if !runs_in_python_environment(command_line) {
        return None;
    }
    let components: Vec<&str> = working_directory.split(['/', '\\']).filter(|c| !c.is_empty() && *c != ".").collect();
    if components.contains(&"..") || Path::new(working_directory).is_absolute() {
        return None;
    }
    let path = if components.is_empty() { ".".to_string() } else { components.join("/") };
    let directory = ScannedDirectory { root: standardize(Path::new(&expand_tilde(project_path))), path, syntax };
    nearest(&directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_which_command_lines_run_in_a_python_environment() {
        for command_line in [
            "uvicorn src.api.main:app --reload",
            "python manage.py runserver",
            "python3.12 -m http.server",
            "py manage.py runserver %PORT%",
            "DEBUG=1 PATH=/opt/bin flask --app app run",
            "gunicorn app:app",
            "./manage.py runserver",
            "celery -A proj worker",
        ] {
            assert!(runs_in_python_environment(command_line), "{command_line}");
        }
        for command_line in [
            ".venv/bin/uvicorn app:app",
            r".venv\Scripts\uvicorn app:app",
            "virtualenv/bin/python manage.py runserver",
            "source virtualenv/bin/activate && uvicorn app:app",
            "uv run uvicorn app:app",
            "poetry run python manage.py runserver",
            "pipenv run flask run",
            "pnpm dev",
            "pythonista",
            "",
        ] {
            assert!(!runs_in_python_environment(command_line), "{command_line}");
        }
    }
}
