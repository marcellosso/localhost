//! Edits one table of a TOML configuration file (Codex's `config.toml`) in
//! place, every other line as it was. It knows only what that takes: where
//! tables and keys start, outside strings and multi-line arrays, and the
//! values of a server's `command` and `args`.

/// A line of the file, and what starts on it.
struct Line {
    start: usize,
    end: usize,
    kind: LineKind,
}

enum LineKind {
    /// `[a.b]` or `[[a.b]]`, as its path.
    Header(Vec<String>),
    /// `a.b = …` (its key's path), up to the line `end_line` its value ends on.
    Key { path: Vec<String>, value_start: usize, end_line: usize },
    /// Blank, a comment, or inside a multi-line value.
    Other,
}

/// The table `path` (e.g. `["mcp_servers", "localhost"]`) as written in a file.
pub(super) struct Table {
    /// Its `key = value` lines, as written.
    entries: Vec<(String, String)>,
}

impl Table {
    /// The string value of `key`.
    pub(super) fn string(&self, key: &str) -> Option<String> {
        let (_, raw) = self.entries.iter().find(|(name, _)| name == key)?;
        let mut parser = ValueParser { text: raw.as_str(), position: 0 };
        let value = parser.string()?;
        parser.end().then_some(value)
    }

    /// The value of `key` when it is an array of strings.
    pub(super) fn strings(&self, key: &str) -> Option<Vec<String>> {
        let (_, raw) = self.entries.iter().find(|(name, _)| name == key)?;
        let mut parser = ValueParser { text: raw.as_str(), position: 0 };
        let value = parser.strings()?;
        parser.end().then_some(value)
    }
}

/// The table at `path`, written as `[path]`. `Err` when the file can't be
/// read, or defines the table another way (inline, or with dotted keys),
/// which this doesn't edit.
pub(super) fn table(text: &str, path: &[&str]) -> Result<Option<Table>, String> {
    let lines = lines(text)?;
    check_written_as_tables(&lines, path)?;
    let Some(header) = lines.iter().position(|line| matches!(&line.kind, LineKind::Header(header) if is(header, path)))
    else {
        return Ok(None);
    };
    let mut entries = Vec::new();
    for line in &lines[header + 1..] {
        match &line.kind {
            LineKind::Header(_) => break,
            LineKind::Key { path, value_start, end_line } if path.len() == 1 => {
                entries.push((path[0].clone(), text[*value_start..lines[*end_line].end].trim().to_string()));
            }
            _ => {}
        }
    }
    Ok(Some(Table { entries }))
}

/// `text` with the table at `path` holding `entries` (key, rendered value):
/// each one replaced where it is, or added under the header; the table
/// added at the end if there is none. Its other keys stay.
pub(super) fn set(text: &str, path: &[&str], entries: &[(&str, String)]) -> Result<String, String> {
    let lines = lines(text)?;
    check_written_as_tables(&lines, path)?;
    let Some(header) = lines.iter().position(|line| matches!(&line.kind, LineKind::Header(header) if is(header, path)))
    else {
        let mut output = text.to_string();
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        if !output.trim().is_empty() && !output.ends_with("\n\n") {
            output.push('\n');
        }
        output.push_str(&format!("[{}]\n", render_path(path)));
        for (key, value) in entries {
            output.push_str(&format!("{key} = {value}\n"));
        }
        return Ok(output);
    };
    let end = lines[header + 1..]
        .iter()
        .position(|line| matches!(line.kind, LineKind::Header(_)))
        .map_or(lines.len(), |offset| header + 1 + offset);
    // Edits from the bottom up, so earlier offsets stay right.
    let mut replacements: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut missing = Vec::new();
    for (key, value) in entries {
        let found = (header + 1..end).find_map(|index| match &lines[index].kind {
            LineKind::Key { path, value_start, end_line } if path.len() == 1 && path[0] == *key => {
                Some((index, *value_start, *end_line))
            }
            _ => None,
        });
        let Some((index, value_start, end_line)) = found else {
            missing.push(format!("{key} = {value}\n"));
            continue;
        };
        let rest = &text[value_start..lines[end_line].end];
        let newline = if rest.ends_with('\n') { "\n" } else { "" };
        // A comment after a one-line value stays; one inside a multi-line array goes with it.
        let comment =
            if end_line == index { rest[strip_comment(rest).len()..].trim_end_matches(['\r', '\n']) } else { "" };
        let spacing = if comment.is_empty() { "" } else { " " };
        replacements.push((value_start..lines[end_line].end, format!("{value}{spacing}{comment}{newline}")));
    }
    if !missing.is_empty() {
        let at = lines[header].end;
        let prefix = if text[..at].ends_with('\n') { "" } else { "\n" };
        replacements.push((at..at, format!("{prefix}{}", missing.concat())));
    }
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut output = text.to_string();
    for (range, replacement) in replacements {
        output.replace_range(range, &replacement);
    }
    Ok(output)
}

/// `text` without the table at `path` and its sub-tables (`[path.env]`), or
/// `None` when there is none.
pub(super) fn remove(text: &str, path: &[&str]) -> Result<Option<String>, String> {
    let lines = lines(text)?;
    check_written_as_tables(&lines, path)?;
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if matches!(&lines[index].kind, LineKind::Header(header) if starts_with(header, path)) {
            let end = lines[index + 1..]
                .iter()
                .position(|line| matches!(line.kind, LineKind::Header(_)))
                .map_or(lines.len(), |offset| index + 1 + offset);
            ranges.push(lines[index].start..lines.get(end).map_or(text.len(), |line| line.start));
            index = end;
        } else {
            index += 1;
        }
    }
    if ranges.is_empty() {
        return Ok(None);
    }
    let mut output = text.to_string();
    for range in ranges.into_iter().rev() {
        output.replace_range(range, "");
    }
    // The blank line that separated it from the table before, at the end.
    if output.ends_with("\n\n") && !text.ends_with("\n\n") {
        output.pop();
    }
    Ok(Some(output))
}

/// A TOML basic string.
pub(super) fn render_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\t' => output.push_str("\\t"),
            '\r' => output.push_str("\\r"),
            character if character.is_control() => output.push_str(&format!("\\u{:04X}", character as u32)),
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

pub(super) fn render_strings(values: &[&str]) -> String {
    format!("[{}]", values.iter().map(|value| render_string(value)).collect::<Vec<_>>().join(", "))
}

fn render_path(path: &[&str]) -> String {
    path.iter()
        .map(|key| {
            if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                key.to_string()
            } else {
                render_string(key)
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn starts_with(header: &[String], path: &[&str]) -> bool {
    header.len() >= path.len() && header.iter().zip(path).all(|(a, b)| a == b)
}

fn is(header: &[String], path: &[&str]) -> bool {
    header.len() == path.len() && header.iter().zip(path).all(|(a, b)| a == b)
}

/// Fails when a key outside `[path]` tables reaches into `path`: an inline
/// table (`localhost = { … }` under `[mcp_servers]`), or dotted keys.
fn check_written_as_tables(lines: &[Line], path: &[&str]) -> Result<(), String> {
    let mut table: &[String] = &[];
    for line in lines {
        match &line.kind {
            LineKind::Header(header) => table = header,
            LineKind::Key { path: key, .. } => {
                let full: Vec<String> = table.iter().chain(key).cloned().collect();
                let overlap = full.len().min(path.len());
                if !starts_with(table, path) && full[..overlap].iter().zip(path).all(|(a, b)| a == b) {
                    return Err(format!(
                        "it sets “{}” another way than as a [{}] table",
                        full.join("."),
                        path.join(".")
                    ));
                }
            }
            LineKind::Other => {}
        }
    }
    Ok(())
}

/// `value` up to a `#` comment outside strings, with the line break.
fn strip_comment(value: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if character == '\\' => escaped = true,
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '#' => return &value[..index],
            None => {}
        }
    }
    value.trim_end_matches(['\r', '\n'])
}

/// Splits `text` into lines, finding headers and keys.
fn lines(text: &str) -> Result<Vec<Line>, String> {
    let mut lines: Vec<Line> = Vec::new();
    let mut state = State::default();
    let mut open_key: Option<usize> = None;
    let mut start = 0;
    while start < text.len() {
        let end = text[start..].find('\n').map_or(text.len(), |index| start + index + 1);
        let line = &text[start..end];
        let number = lines.len();
        let kind = if state.is_clean() {
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if trimmed.starts_with('[') {
                LineKind::Header(
                    header(trimmed).ok_or_else(|| format!("line {} isn’t a table header TOML can read", number + 1))?,
                )
            } else if trimmed.is_empty() || trimmed.starts_with('#') {
                LineKind::Other
            } else {
                let (path, after) =
                    key(trimmed).ok_or_else(|| format!("line {} isn’t a key TOML can read", number + 1))?;
                let value_start = start + indent + (trimmed.len() - after.len());
                state.scan(after);
                if !state.is_clean() {
                    open_key = Some(number);
                }
                LineKind::Key { path, value_start, end_line: number }
            }
        } else {
            state.scan(line);
            if state.is_clean()
                && let Some(key) = open_key.take()
                && let LineKind::Key { end_line, .. } = &mut lines[key].kind
            {
                *end_line = number;
            }
            LineKind::Other
        };
        lines.push(Line { start, end, kind });
        start = end;
    }
    if !state.is_clean() {
        return Err("a string or an array isn’t closed".into());
    }
    Ok(lines)
}

/// Where a scan is between lines: inside a multi-line string, or how deep
/// in brackets.
#[derive(Default)]
struct State {
    multiline: Option<&'static str>,
    depth: usize,
}

impl State {
    fn is_clean(&self) -> bool {
        self.multiline.is_none() && self.depth == 0
    }

    /// Follows strings, comments and brackets to the end of `line`.
    fn scan(&mut self, line: &str) {
        let mut rest = line;
        while !rest.is_empty() {
            if let Some(close) = self.multiline {
                match rest.find(close) {
                    Some(index) if close == "\"\"\"" && is_escaped(&rest[..index]) => {
                        rest = &rest[index + 1..];
                    }
                    Some(index) => {
                        self.multiline = None;
                        rest = &rest[index + 3..];
                    }
                    None => return,
                }
                continue;
            }
            if let Some(after) = rest.strip_prefix("\"\"\"") {
                self.multiline = Some("\"\"\"");
                rest = after;
            } else if let Some(after) = rest.strip_prefix("'''") {
                self.multiline = Some("'''");
                rest = after;
            } else if rest.starts_with('"') || rest.starts_with('\'') {
                let quote = rest.as_bytes()[0];
                let bytes = rest.as_bytes();
                let mut index = 1;
                while index < bytes.len() && bytes[index] != quote {
                    index += if quote == b'"' && bytes[index] == b'\\' { 2 } else { 1 };
                }
                rest = &rest[(index + 1).min(rest.len())..];
            } else if rest.starts_with('#') {
                return;
            } else {
                match rest.as_bytes()[0] {
                    b'[' | b'{' => self.depth += 1,
                    b']' | b'}' => self.depth = self.depth.saturating_sub(1),
                    _ => {}
                }
                let next = rest.chars().next().map_or(1, char::len_utf8);
                rest = &rest[next..];
            }
        }
    }
}

/// Whether the text before a `"""` ends in an odd number of backslashes.
fn is_escaped(before: &str) -> bool {
    before.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1
}

/// `[a."b c".d]` → `["a", "b c", "d"]`; `[[a]]` too.
fn header(line: &str) -> Option<Vec<String>> {
    let double = line.starts_with("[[");
    let inner = &line[if double { 2 } else { 1 }..];
    let (path, rest) = dotted_key(inner)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(if double { "]]" } else { "]" })?;
    let rest = rest.trim();
    (rest.is_empty() || rest.starts_with('#')).then_some(path)
}

/// `a.b = value` → (`["a", "b"]`, `value…`).
fn key(line: &str) -> Option<(Vec<String>, &str)> {
    let (path, rest) = dotted_key(line)?;
    let rest = rest.trim_start().strip_prefix('=')?;
    Some((path, rest.trim_start_matches([' ', '\t'])))
}

fn dotted_key(text: &str) -> Option<(Vec<String>, &str)> {
    let mut path = Vec::new();
    let mut rest = text.trim_start_matches([' ', '\t']);
    loop {
        let (part, after) = if rest.starts_with('"') || rest.starts_with('\'') {
            let mut parser = ValueParser { text: rest, position: 0 };
            let part = parser.string()?;
            (part, &rest[parser.position..])
        } else {
            let length =
                rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).unwrap_or(rest.len());
            if length == 0 {
                return None;
            }
            (rest[..length].to_string(), &rest[length..])
        };
        path.push(part);
        let after = after.trim_start_matches([' ', '\t']);
        match after.strip_prefix('.') {
            Some(next) => rest = next.trim_start_matches([' ', '\t']),
            None => return Some((path, after)),
        }
    }
}

/// Reads the few TOML values a server entry needs: strings and arrays of them.
struct ValueParser<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> ValueParser<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.position..]
    }

    fn skip_space(&mut self) {
        loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.position += rest.len() - trimmed.len();
            if trimmed.starts_with('#') {
                self.position += trimmed.find('\n').unwrap_or(trimmed.len());
            } else {
                return;
            }
        }
    }

    fn end(&mut self) -> bool {
        self.skip_space();
        self.rest().is_empty()
    }

    fn string(&mut self) -> Option<String> {
        self.skip_space();
        let rest = self.rest();
        if let Some(literal) = rest.strip_prefix('\'') {
            let end = literal.find('\'')?;
            self.position += end + 2;
            return Some(literal[..end].to_string());
        }
        let mut characters = rest.strip_prefix('"')?.char_indices();
        let mut value = String::new();
        while let Some((index, character)) = characters.next() {
            match character {
                '"' => {
                    self.position += index + 2;
                    return Some(value);
                }
                '\\' => match characters.next()?.1 {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    'b' => value.push('\u{8}'),
                    'f' => value.push('\u{c}'),
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    marker @ ('u' | 'U') => {
                        let length = if marker == 'u' { 4 } else { 8 };
                        let hex: String = (0..length).filter_map(|_| characters.next().map(|(_, c)| c)).collect();
                        value.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    _ => return None,
                },
                character => value.push(character),
            }
        }
        None
    }

    fn strings(&mut self) -> Option<Vec<String>> {
        self.skip_space();
        if !self.rest().starts_with('[') {
            return None;
        }
        self.position += 1;
        let mut values = Vec::new();
        loop {
            self.skip_space();
            if self.rest().starts_with(']') {
                self.position += 1;
                return Some(values);
            }
            values.push(self.string()?);
            self.skip_space();
            if self.rest().starts_with(',') {
                self.position += 1;
            } else if !self.rest().starts_with(']') {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: [&str; 2] = ["mcp_servers", "localhost"];

    fn entries() -> Vec<(&'static str, String)> {
        vec![("command", render_string("/opt/local host/localhost")), ("args", render_strings(&["mcp"]))]
    }

    #[test]
    fn adds_a_table_at_the_end_and_removes_it_again() {
        let text = "model = \"gpt-5\"\n\n[mcp_servers.github]\ncommand = \"gh\"\n";
        let edited = set(text, &PATH, &entries()).unwrap();
        assert_eq!(
            edited,
            "model = \"gpt-5\"\n\n[mcp_servers.github]\ncommand = \"gh\"\n\n[mcp_servers.localhost]\ncommand = \"/opt/local host/localhost\"\nargs = [\"mcp\"]\n"
        );
        let table = table(&edited, &PATH).unwrap().unwrap();
        assert_eq!(table.string("command").as_deref(), Some("/opt/local host/localhost"));
        assert_eq!(table.strings("args"), Some(vec!["mcp".to_string()]));
        assert_eq!(remove(&edited, &PATH).unwrap().unwrap(), text);
    }

    #[test]
    fn starts_an_empty_file() {
        assert_eq!(
            set("", &PATH, &entries()).unwrap(),
            "[mcp_servers.localhost]\ncommand = \"/opt/local host/localhost\"\nargs = [\"mcp\"]\n"
        );
        assert_eq!(
            set("a = 1", &PATH, &entries()).unwrap(),
            "a = 1\n\n[mcp_servers.localhost]\ncommand = \"/opt/local host/localhost\"\nargs = [\"mcp\"]\n"
        );
    }

    #[test]
    fn repoints_a_table_keeping_its_other_keys_and_comments() {
        let text = "[mcp_servers.localhost] # mine\ncommand = '/Users/me/.cargo/bin/localhost' # built by hand\nargs = [\n  \"mcp\",\n  # more\n  \"--verbose\",\n]\nstartup_timeout_sec = 20\n\n[mcp_servers.localhost.env]\nA = \"1\"\n\n[profiles.fast]\nmodel = \"o4\"\n";
        let edited = set(text, &PATH, &entries()).unwrap();
        assert_eq!(
            edited,
            "[mcp_servers.localhost] # mine\ncommand = \"/opt/local host/localhost\" # built by hand\nargs = [\"mcp\"]\nstartup_timeout_sec = 20\n\n[mcp_servers.localhost.env]\nA = \"1\"\n\n[profiles.fast]\nmodel = \"o4\"\n"
        );
        assert_eq!(remove(&edited, &PATH).unwrap().unwrap(), "[profiles.fast]\nmodel = \"o4\"\n");
    }

    #[test]
    fn adds_missing_keys_under_the_header() {
        let text = "[mcp_servers.localhost]\nenabled = true\n";
        assert_eq!(
            set(text, &PATH, &entries()).unwrap(),
            "[mcp_servers.localhost]\ncommand = \"/opt/local host/localhost\"\nargs = [\"mcp\"]\nenabled = true\n"
        );
    }

    #[test]
    fn ignores_brackets_in_strings_and_arrays() {
        let text = "notes = \"\"\"\n[mcp_servers.localhost]\n\"\"\"\nmatrix = [\n  [1, 2],\n]\nname = \"[x]\"\n";
        assert!(table(text, &PATH).unwrap().is_none());
        assert_eq!(remove(text, &PATH).unwrap(), None);
        assert!(set(text, &PATH, &entries()).unwrap().starts_with(text));
    }

    #[test]
    fn refuses_a_server_written_another_way() {
        for text in [
            "[mcp_servers]\nlocalhost = { command = \"x\" }\n",
            "mcp_servers.localhost.command = \"x\"\n",
            "mcp_servers = { localhost = { command = \"x\" } }\n",
            "[mcp_servers]\n\"localhost\".command = \"x\"\n",
        ] {
            assert!(set(text, &PATH, &entries()).is_err(), "{text}");
            assert!(table(text, &PATH).is_err(), "{text}");
        }
        assert!(set("[mcp_servers]\ngithub = { command = \"gh\" }\n", &PATH, &entries()).is_ok());
    }

    #[test]
    fn reads_quoted_and_escaped_values() {
        let text = "[mcp_servers.\"localhost\"]\ncommand = \"C:\\\\Program Files\\\\localhost.exe\"\nargs = ['mcp']\n";
        let table = table(text, &PATH).unwrap().unwrap();
        assert_eq!(table.string("command").as_deref(), Some(r"C:\Program Files\localhost.exe"));
        assert_eq!(table.strings("args"), Some(vec!["mcp".to_string()]));
        assert_eq!(render_string(r"C:\a"), r#""C:\\a""#);
    }

    #[test]
    fn refuses_what_it_cant_read() {
        assert!(set("a = \"\"\"\nopen", &PATH, &entries()).is_err());
        assert!(set("[unclosed\n", &PATH, &entries()).is_err());
    }
}
