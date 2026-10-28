//! The command line. Six commands with positional arguments only, so it is
//! parsed by hand rather than with an argument-parsing crate.

use std::ffi::OsString;

pub const USAGE: &str = "\
Usage: localhost <command>

Commands:
  projects                   List every Project
  status <project>           Show which Services of a Project are running
  logs <project> [service]   Print the output of the latest Run of a Project's Services
  up <project> [service…]    Run a Project's Services, or some of them with their
                             Dependencies, until Ctrl-C stops them all
  mcp                        Serve Projects to a coding agent over MCP, on stdin
                             and stdout (see https://www.getlocalhost.app/docs/mcp)

A Project or Service is named by its Key, its name, or letters of either in
order: `localhost up ashop web` finds Acme Shop and its Service web.

Options:
  -h, --help       Show this help
  -V, --version    Show the version";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Projects,
    Status {
        project: String,
    },
    Logs {
        project: String,
        service: Option<String>,
    },
    Up {
        project: String,
        services: Vec<String>,
    },
    Mcp,
    /// The shared core of a storage root, which `localhost mcp` and the
    /// app start when it isn't running: not for people, so not in the usage.
    Daemon {
        storage_root: String,
    },
}

/// Reads the arguments after the program name. `Err` says what is wrong.
pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let arguments: Vec<String> = arguments
        .into_iter()
        .map(|argument| argument.into_string().map_err(|raw| format!("{} is not valid UTF-8.", raw.display())))
        .collect::<Result<_, _>>()?;
    let Some((command, rest)) = arguments.split_first() else {
        return Err(format!("{USAGE}\n"));
    };
    let rest: Vec<String> = rest.to_vec();
    match (command.as_str(), rest.as_slice()) {
        ("-h" | "--help" | "help", []) => Ok(Command::Help),
        ("-V" | "--version", []) => Ok(Command::Version),
        ("projects", []) => Ok(Command::Projects),
        ("mcp", []) => Ok(Command::Mcp),
        ("daemon", [storage_root]) => Ok(Command::Daemon { storage_root: storage_root.clone() }),
        ("status", [project]) => Ok(Command::Status { project: project.clone() }),
        ("logs", [project]) => Ok(Command::Logs { project: project.clone(), service: None }),
        ("logs", [project, service]) => Ok(Command::Logs { project: project.clone(), service: Some(service.clone()) }),
        ("up", [project, services @ ..]) => Ok(Command::Up { project: project.clone(), services: services.to_vec() }),
        ("projects" | "mcp" | "-h" | "--help" | "help" | "-V" | "--version", _) => {
            Err(format!("`{command}` takes no arguments."))
        }
        ("status", _) => Err("Usage: localhost status <project>".into()),
        ("logs", _) => Err("Usage: localhost logs <project> [service]".into()),
        ("up", _) => Err("Usage: localhost up <project> [service…]".into()),
        ("daemon", _) => Err("Usage: localhost daemon <storage root>".into()),
        _ => Err(format!("Unknown command `{command}`.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(arguments: &[&str]) -> Result<Command, String> {
        parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn reads_each_command_and_its_arguments() {
        assert_eq!(parsed(&["projects"]), Ok(Command::Projects));
        assert_eq!(parsed(&["status", "acme"]), Ok(Command::Status { project: "acme".into() }));
        assert_eq!(parsed(&["logs", "acme"]), Ok(Command::Logs { project: "acme".into(), service: None }));
        assert_eq!(
            parsed(&["logs", "acme", "web"]),
            Ok(Command::Logs { project: "acme".into(), service: Some("web".into()) })
        );
        assert_eq!(parsed(&["up", "acme"]), Ok(Command::Up { project: "acme".into(), services: vec![] }));
        assert_eq!(
            parsed(&["up", "acme", "web", "api"]),
            Ok(Command::Up { project: "acme".into(), services: vec!["web".into(), "api".into()] })
        );
        assert_eq!(parsed(&["mcp"]), Ok(Command::Mcp));
        assert_eq!(parsed(&["daemon", "/tmp/root"]), Ok(Command::Daemon { storage_root: "/tmp/root".into() }));
        assert_eq!(parsed(&["--help"]), Ok(Command::Help));
        assert_eq!(parsed(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn names_may_contain_spaces_and_look_like_options() {
        assert_eq!(parsed(&["status", "Acme Shop"]), Ok(Command::Status { project: "Acme Shop".into() }));
        assert_eq!(parsed(&["status", "-x"]), Ok(Command::Status { project: "-x".into() }));
    }

    #[test]
    fn rejects_unknown_commands_and_wrong_argument_counts() {
        assert_eq!(parsed(&["start", "acme"]), Err("Unknown command `start`.".into()));
        assert_eq!(parsed(&["status"]), Err("Usage: localhost status <project>".into()));
        assert_eq!(parsed(&["logs", "a", "b", "c"]), Err("Usage: localhost logs <project> [service]".into()));
        assert_eq!(parsed(&["up"]), Err("Usage: localhost up <project> [service…]".into()));
        assert_eq!(parsed(&["projects", "extra"]), Err("`projects` takes no arguments.".into()));
        assert!(parsed(&[]).unwrap_err().starts_with("Usage: localhost <command>"));
    }
}
