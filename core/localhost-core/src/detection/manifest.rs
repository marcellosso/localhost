//! Small readers for the manifest files detection looks at. Each reads just
//! enough of its format and skips what it doesn't understand rather than
//! failing the scan.

use std::collections::HashMap;

/// Just enough TOML to read `pyproject.toml` and `Cargo.toml`: tables, array
/// tables and `key = value` pairs whose values stay raw.
pub(crate) struct TomlDocument {
    /// In file order; the top level is a table named "".
    pub tables: Vec<TomlTable>,
}

pub(crate) struct TomlTable {
    /// e.g. `package`, `tool.poetry`, `bin` for `[[bin]]`.
    pub name: String,
    pub values: HashMap<String, String>,
}

impl TomlDocument {
    pub fn parse(text: &str) -> TomlDocument {
        let mut tables = vec![TomlTable { name: String::new(), values: HashMap::new() }];
        let mut pending: Option<(String, String)> = None;
        for raw_line in text.split('\n') {
            let line = toml_stripping_comment(raw_line).trim().to_string();
            if let Some((key, value)) = pending.take() {
                let joined = format!("{value} {line}");
                if is_balanced(&joined) {
                    tables.last_mut().unwrap().values.insert(key, joined);
                } else {
                    pending = Some((key, joined));
                }
                continue;
            }
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                let name = line.trim_matches(|c| c == '[' || c == ']' || c == ' ').to_string();
                tables.push(TomlTable { name, values: HashMap::new() });
                continue;
            }
            let Some(equals) = line.find('=') else { continue };
            let key = line[..equals].trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            let value = line[equals + 1..].trim().to_string();
            if is_balanced(&value) {
                tables.last_mut().unwrap().values.insert(key, value);
            } else {
                pending = Some((key, value));
            }
        }
        TomlDocument { tables }
    }

    pub fn table(&self, name: &str) -> Option<&TomlTable> {
        self.tables.iter().find(|table| table.name == name)
    }

    pub fn tables_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a TomlTable> {
        self.tables.iter().filter(move |table| table.name == name)
    }

    /// A raw value as a string, if it is a quoted string.
    pub fn string(raw: Option<&String>) -> Option<String> {
        let raw = raw?;
        let quote = raw.chars().next()?;
        if raw.chars().count() < 2 || !(quote == '"' || quote == '\'') || !raw.ends_with(quote) {
            return None;
        }
        Some(raw[1..raw.len() - 1].to_string())
    }

    /// The strings of a raw array value such as `["a", 'b/*']`.
    pub fn strings(raw: Option<&String>) -> Vec<String> {
        match raw {
            Some(raw) if raw.starts_with('[') => quoted_strings(raw),
            _ => Vec::new(),
        }
    }
}

fn is_balanced(value: &str) -> bool {
    value.matches('[').count() <= value.matches(']').count()
}

fn toml_stripping_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (index, c) in line.char_indices() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return &line[..index],
            None => {}
        }
    }
    line
}

/// The quoted strings in a line, e.g. `["a", 'b']` → `a`, `b`.
pub(crate) fn quoted_strings(text: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match quote {
            Some(open) if c == open => {
                strings.push(std::mem::take(&mut current));
                quote = None;
            }
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => quote = Some(c),
            None => {}
        }
    }
    strings
}

/// Just enough YAML for workspace files and Compose files: block mappings by
/// indentation, block and flow sequences of scalars. Anchors, multi-line
/// scalars and other features are skipped rather than failing the scan.
pub(crate) struct YamlLines {
    pub lines: Vec<YamlLine>,
}

#[derive(Clone, Debug)]
pub(crate) struct YamlLine {
    /// 1-based, in the original text.
    #[allow(dead_code)]
    pub number: usize,
    pub indent: usize,
    /// Without indentation, comment and trailing spaces.
    pub text: String,
}

impl YamlLine {
    pub fn is_list_item(&self) -> bool {
        self.text == "-" || self.text.starts_with("- ")
    }

    /// For `key: value` (or `key:`), the unquoted key and the raw value.
    pub fn key_value(&self) -> Option<(String, String)> {
        let body = if self.is_list_item() { self.text.get(2..).unwrap_or("") } else { self.text.as_str() };
        let (key_end, value_start) = match body.find(": ") {
            Some(index) => (index, index + 2),
            None if body.ends_with(':') => (body.len() - 1, body.len()),
            None => return None,
        };
        let key = body[..key_end].trim();
        if key.is_empty() || (key.contains(' ') && !key.starts_with('"') && !key.starts_with('\'')) {
            return None;
        }
        Some((unquoted(key).to_string(), body[value_start..].trim().to_string()))
    }
}

impl YamlLines {
    pub fn parse(text: &str) -> YamlLines {
        let lines = text
            .split('\n')
            .enumerate()
            .filter_map(|(offset, raw)| {
                let leading: &str = &raw[..raw.len() - raw.trim_start_matches([' ', '\t']).len()];
                let indent = leading.chars().map(|c| if c == '\t' { 2 } else { 1 }).sum();
                let text = yaml_stripping_comment(&raw[leading.len()..]).trim().to_string();
                (!text.is_empty() && text != "---").then_some(YamlLine { number: offset + 1, indent, text })
            })
            .collect();
        YamlLines { lines }
    }

    /// The lines nested under the top-level `key:`.
    pub fn block(&self, key: &str) -> &[YamlLine] {
        let Some(start) =
            self.lines.iter().position(|line| line.indent == 0 && line.key_value().is_some_and(|(k, _)| k == key))
        else {
            return &[];
        };
        let end = self.lines[start + 1..]
            .iter()
            .position(|line| line.indent == 0 && !line.is_list_item())
            .map_or(self.lines.len(), |offset| start + 1 + offset);
        &self.lines[start + 1..end]
    }

    /// The scalars of a sequence: either a flow sequence value (`[a, b]`) or
    /// the `- item` lines that follow.
    pub fn sequence(value: &str, following: &[YamlLine]) -> Vec<String> {
        if let Some(inner) = value.strip_prefix('[') {
            let inner = inner.strip_suffix(']').unwrap_or(&inner[..inner.len().saturating_sub(1)]);
            return inner.split(',').map(|item| unquoted(item.trim()).to_string()).filter(|s| !s.is_empty()).collect();
        }
        let Some(first) = following.first() else { return Vec::new() };
        if !value.is_empty() || !first.is_list_item() {
            return Vec::new();
        }
        following
            .iter()
            .take_while(|line| line.indent == first.indent && line.is_list_item())
            .map(|line| unquoted(line.text.get(2..).unwrap_or("").trim()).to_string())
            .collect()
    }
}

pub(crate) fn unquoted(text: &str) -> &str {
    let mut chars = text.chars();
    match (chars.next(), text.chars().last()) {
        (Some(first), Some(last)) if text.len() >= 2 && (first == '"' || first == '\'') && last == first => {
            &text[1..text.len() - 1]
        }
        _ => text,
    }
}

/// A quote only opens a scalar (`key: "a # b"`, `[a, 'b']`), not inside one
/// (`echo it's # comment`); `\"` doesn't close a double-quoted one.
fn yaml_stripping_comment(text: &str) -> &str {
    if text.starts_with('#') {
        return "";
    }
    let mut quote: Option<char> = None;
    let mut is_escaped = false;
    let mut previous = ' ';
    for (index, c) in text.char_indices() {
        if let Some(open) = quote {
            if is_escaped {
                is_escaped = false;
            } else if open == '"' && c == '\\' {
                is_escaped = true;
            } else if c == open {
                quote = None;
            }
        } else if (c == '"' || c == '\'') && " [{,".contains(previous) {
            quote = Some(c);
        } else if c == '#' && previous == ' ' {
            return &text[..index];
        }
        previous = c;
    }
    text
}

/// Quotes a word for the login shell when it isn't plainly safe.
pub(crate) fn shell_quoted(word: &str) -> String {
    let is_safe = |c: char| c.is_ascii_alphanumeric() || "@%+=:,./_-".contains(c);
    if !word.is_empty() && word.chars().all(is_safe) {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toml_reader_keeps_tables_array_tables_and_multi_line_arrays() {
        let document = TomlDocument::parse(
            "name = \"top\"\n[package]\nname = \"x\" # comment\n[[bin]]\nname = 'a'\n[[bin]]\nname = \"b\"\n\
             [workspace]\nmembers = [\n  \"one\", # first\n  \"two/*\",\n]",
        );
        assert_eq!(TomlDocument::string(document.table("").unwrap().values.get("name")).as_deref(), Some("top"));
        assert_eq!(TomlDocument::string(document.table("package").unwrap().values.get("name")).as_deref(), Some("x"));
        let bins: Vec<String> =
            document.tables_named("bin").filter_map(|table| TomlDocument::string(table.values.get("name"))).collect();
        assert_eq!(bins, ["a", "b"]);
        assert_eq!(TomlDocument::strings(document.table("workspace").unwrap().values.get("members")), ["one", "two/*"]);
    }

    #[test]
    fn yaml_comments_stop_at_a_quote_only_where_a_scalar_starts() {
        let lines = YamlLines::parse("a: \"x # y\" # gone\nb: it's # gone\n# whole line\nc: 'q' # gone\n");
        let texts: Vec<&str> = lines.lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts, ["a: \"x # y\"", "b: it's", "c: 'q'"]);
    }

    #[test]
    fn quotes_only_what_needs_it() {
        assert_eq!(shell_quoted("dev:web"), "dev:web");
        assert_eq!(shell_quoted("do it"), "'do it'");
        assert_eq!(shell_quoted("it's"), r"'it'\''s'");
        assert_eq!(shell_quoted(""), "''");
    }
}
