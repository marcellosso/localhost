/// The shell a suggested command line is written for: the login shell on
/// macOS and Linux (ADR-0003), `cmd.exe` on Windows (ADR-0010).
///
/// Scanning runs where the Project runs (a WSL Project is scanned by the core
/// inside the distro), so the syntax is this build's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandSyntax {
    Posix,
    Cmd,
}

impl CommandSyntax {
    /// This machine's shell.
    pub const HERE: CommandSyntax = if cfg!(windows) { CommandSyntax::Cmd } else { CommandSyntax::Posix };

    /// The Port variable (ADR-0006): `$PORT`, or `%PORT%` for cmd.
    pub fn port(self) -> &'static str {
        match self {
            CommandSyntax::Posix => "$PORT",
            CommandSyntax::Cmd => "%PORT%",
        }
    }

    /// A word quoted when it isn't plainly safe: single quotes for the login
    /// shell, double quotes for cmd (which has no way to escape `%` inside
    /// them, so a name with `%` may still expand a variable).
    pub fn quoted(self, word: &str) -> String {
        let is_safe = |c: char| {
            c.is_ascii_alphanumeric() || "@+=:,./_-".contains(c) || (c == '%' && self == CommandSyntax::Posix)
        };
        if !word.is_empty() && word.chars().all(is_safe) {
            return word.to_string();
        }
        match self {
            CommandSyntax::Posix => format!("'{}'", word.replace('\'', r"'\''")),
            CommandSyntax::Cmd => format!("\"{}\"", word.replace('"', "\\\"")),
        }
    }

    /// A relative program path, e.g. `.venv/bin/python`. cmd reads a `/` in
    /// the program name as the start of a switch, so its paths use `\`.
    pub fn program_path(self, components: &[&str]) -> String {
        match self {
            CommandSyntax::Posix => components.join("/"),
            CommandSyntax::Cmd => components.join("\\"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_what_needs_it() {
        let posix = |word| CommandSyntax::Posix.quoted(word);
        assert_eq!(posix("dev:web"), "dev:web");
        assert_eq!(posix("do it"), "'do it'");
        assert_eq!(posix("it's"), r"'it'\''s'");
        assert_eq!(posix(""), "''");
        let cmd = |word| CommandSyntax::Cmd.quoted(word);
        assert_eq!(cmd("dev:web"), "dev:web");
        assert_eq!(cmd("do it"), "\"do it\"");
        assert_eq!(cmd("a&b"), "\"a&b\"");
        assert_eq!(cmd("100%"), "\"100%\"");
        assert_eq!(cmd("say \"hi\""), r#""say \"hi\"""#);
        assert_eq!(cmd(""), "\"\"");
    }

    #[test]
    fn writes_the_port_and_program_paths_for_each_shell() {
        assert_eq!(CommandSyntax::Posix.port(), "$PORT");
        assert_eq!(CommandSyntax::Cmd.port(), "%PORT%");
        assert_eq!(CommandSyntax::Posix.program_path(&[".venv", "bin", "python"]), ".venv/bin/python");
        assert_eq!(CommandSyntax::Cmd.program_path(&[".venv", "Scripts", "python"]), r".venv\Scripts\python");
    }
}
