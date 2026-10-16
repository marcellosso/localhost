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
}
