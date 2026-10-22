//! Writing to the terminal: colour only when stdout is one and `NO_COLOR` is
//! unset, and no panic when the reader goes away (`localhost logs | head`).

use std::io::{IsTerminal, Write};
use std::time::Duration;

use localhost_core::Ansi;

pub struct Terminal {
    is_styled: bool,
}

impl Terminal {
    pub fn new() -> Terminal {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
        Terminal { is_styled: std::io::stdout().is_terminal() && !no_color }
    }

    /// Writes a line to stdout. A closed pipe is not an error worth reporting.
    pub fn print(&self, line: &str) {
        let _ = writeln!(std::io::stdout().lock(), "{line}");
    }

    pub fn error(&self, message: &str) {
        let _ = writeln!(std::io::stderr().lock(), "{message}");
    }

    /// A Run's output line: as written when colours are shown, else without
    /// its escape sequences.
    pub fn output(&self, text: &str) -> String {
        if self.is_styled { text.to_string() } else { Ansi::strip(text) }
    }

    /// `text` dimmed, when colours are shown.
    pub fn dim(&self, text: &str) -> String {
        self.styled(text, "2")
    }

    pub fn bold(&self, text: &str) -> String {
        self.styled(text, "1")
    }

    fn styled(&self, text: &str, code: &str) -> String {
        if self.is_styled { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
    }

    /// Rows in columns two spaces apart, the first row as the header.
    /// Trailing empty cells are left out.
    pub fn table(&self, rows: &[Vec<String>]) {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        let widths: Vec<usize> = (0..columns)
            .map(|column| {
                rows.iter().filter_map(|row| row.get(column)).map(|cell| cell.chars().count()).max().unwrap_or(0)
            })
            .collect();
        for (index, row) in rows.iter().enumerate() {
            let line: String = row
                .iter()
                .enumerate()
                .map(|(column, cell)| format!("{cell:<width$}", width = widths[column]))
                .collect::<Vec<_>>()
                .join("  ");
            let line = line.trim_end();
            self.print(&if index == 0 { self.dim(line) } else { line.to_string() });
        }
    }
}

/// `path` with the home folder shown as `~`, as the app shows it.
pub fn abbreviated(path: &str) -> String {
    let Some(home) = std::env::var_os("HOME").map(|home| home.to_string_lossy().trim_end_matches('/').to_string())
    else {
        return path.to_string();
    };
    match path.strip_prefix(&home) {
        Some("") if !home.is_empty() => "~".into(),
        Some(rest) if !home.is_empty() && rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// How long something has been up, at most two units: `42s`, `5m 2s`, `3h 4m`, `2d 3h`.
pub fn duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let (days, hours, minutes) = (seconds / 86_400, seconds % 86_400 / 3600, seconds % 3600 / 60);
    match () {
        _ if days > 0 => format!("{days}d {hours}h"),
        _ if hours > 0 => format!("{hours}h {minutes}m"),
        _ if minutes > 0 => format!("{minutes}m {}s", seconds % 60),
        _ => format!("{seconds}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_show_two_units_at_most() {
        assert_eq!(duration(Duration::from_secs(42)), "42s");
        assert_eq!(duration(Duration::from_secs(302)), "5m 2s");
        assert_eq!(duration(Duration::from_secs(3 * 3600 + 4 * 60 + 5)), "3h 4m");
        assert_eq!(duration(Duration::from_secs(2 * 86_400 + 3 * 3600 + 59)), "2d 3h");
    }

    #[test]
    fn abbreviates_the_home_folder_only() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(abbreviated(&format!("{home}/Code/a")), "~/Code/a");
        assert_eq!(abbreviated(&home), "~");
        assert_eq!(abbreviated(&format!("{home}other/a")), format!("{home}other/a"));
        assert_eq!(abbreviated("/tmp/a"), "/tmp/a");
    }
}
