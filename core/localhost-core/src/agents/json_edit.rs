//! Edits a JSON configuration file in place: one member set or removed, every
//! other byte as it was. Agents' files hold far more than the MCP servers
//! (`~/.claude.json` is Claude Code's whole state), so they are never parsed
//! and written back whole, which would reorder and reformat them. Comments
//! and trailing commas, which Cursor tolerates, are skipped like whitespace.

use serde::Serialize;
use serde_json::Value;

/// A member of an object: its key and where its key and value are.
struct Member {
    key: String,
    key_start: usize,
    value_start: usize,
    value_end: usize,
}

/// An object: where its braces are, and its members.
struct Object {
    open: usize,
    close: usize,
    members: Vec<Member>,
}

impl Object {
    fn member(&self, key: &str) -> Option<&Member> {
        self.members.iter().find(|member| member.key == key)
    }
}

/// The value at `path`, e.g. `["mcpServers", "localhost"]`, if there is one.
/// `Err` says why the file can't be read.
pub(super) fn get(text: &str, path: &[&str]) -> Result<Option<Value>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let mut object = root(text)?;
    for (index, key) in path.iter().enumerate() {
        let Some(member) = object.member(key) else { return Ok(None) };
        let slice = &text[member.value_start..member.value_end];
        if index == path.len() - 1 {
            return serde_json::from_str(slice).map(Some).map_err(|error| error.to_string());
        }
        if !slice.starts_with('{') {
            return Ok(None);
        }
        object = Scanner::at(text, member.value_start).object()?;
    }
    Ok(None)
}

/// `text` with the value at `path` set to `value`, creating the objects on
/// the way. Fails when something on the way isn't an object. A struct keeps
/// its fields' order.
pub(super) fn set(text: &str, path: &[&str], value: &impl Serialize) -> Result<String, String> {
    let unit = indent_unit(text);
    if text.trim().is_empty() {
        return Ok(format!("{}\n", render_nested(path, value, "", &unit)));
    }
    let mut object = root(text)?;
    for (index, key) in path.iter().enumerate() {
        let Some(member) = object.member(key) else {
            return Ok(insert(text, &object, key, &path[index + 1..], value, &unit));
        };
        if index == path.len() - 1 {
            let indent = line_indent(text, member.key_start);
            let rendered = render(value, indent, &unit);
            return Ok(format!("{}{rendered}{}", &text[..member.value_start], &text[member.value_end..]));
        }
        if !text[member.value_start..].starts_with('{') {
            return Err(format!("“{key}” isn’t an object"));
        }
        object = Scanner::at(text, member.value_start).object()?;
    }
    unreachable!("the path is never empty")
}

/// `text` without the member at `path`, or `None` when there is none.
pub(super) fn remove(text: &str, path: &[&str]) -> Result<Option<String>, String> {
    let Some((key, parents)) = path.split_last() else { return Ok(None) };
    if text.trim().is_empty() {
        return Ok(None);
    }
    let mut object = root(text)?;
    for parent in parents {
        let Some(member) = object.member(parent) else { return Ok(None) };
        if !text[member.value_start..].starts_with('{') {
            return Ok(None);
        }
        object = Scanner::at(text, member.value_start).object()?;
    }
    let Some(index) = object.members.iter().position(|member| member.key == *key) else { return Ok(None) };
    let member = &object.members[index];
    let range = if object.members.len() == 1 {
        object.open + 1..object.close
    } else if let Some(next) = object.members.get(index + 1) {
        // Up to the next key: its line keeps this member's indentation.
        member.key_start..next.key_start
    } else {
        // The last one: from the end of the one before, with its comma.
        object.members[index - 1].value_end..member.value_end
    };
    Ok(Some(format!("{}{}", &text[..range.start], &text[range.end..])))
}

fn root(text: &str) -> Result<Object, String> {
    let mut scanner = Scanner::at(text, 0);
    scanner.skip_trivia()?;
    if scanner.peek() != Some(b'{') {
        return Err("it isn’t a JSON object".into());
    }
    let object = scanner.object()?;
    scanner.skip_trivia()?;
    if scanner.peek().is_some() {
        return Err("there is more after the JSON object".into());
    }
    Ok(object)
}

/// `{ path[0]: { path[1]: … value } }` rendered, or `value` itself for an empty path.
fn render_nested(path: &[&str], value: &impl Serialize, indent: &str, unit: &str) -> String {
    let Some((key, rest)) = path.split_first() else { return render(value, indent, unit) };
    let inner_indent = format!("{indent}{unit}");
    let inner = render_nested(rest, value, &inner_indent, unit);
    format!("{{\n{inner_indent}{}: {inner}\n{indent}}}", Value::String(key.to_string()))
}

/// `text` with `"key": value` added as the last member of `object` (inside
/// new objects for each of `path`), indented like its other members.
fn insert(text: &str, object: &Object, key: &str, path: &[&str], value: &impl Serialize, unit: &str) -> String {
    let parent_indent = line_indent(text, object.open);
    let key = Value::String(key.to_string());
    let Some(last) = object.members.last() else {
        let indent = format!("{parent_indent}{unit}");
        let rendered = render_nested(path, value, &indent, unit);
        return format!(
            "{}\n{indent}{key}: {rendered}\n{parent_indent}{}",
            &text[..=object.open],
            &text[object.close..]
        );
    };
    let indent = if starts_line(text, last.key_start) {
        line_indent(text, last.key_start).to_string()
    } else {
        format!("{parent_indent}{unit}")
    };
    let rendered = render_nested(path, value, &indent, unit);
    // A trailing comma (JSONC) stays the last thing before the brace.
    let after = text[last.value_end..].trim_start_matches([' ', '\t', '\r', '\n']);
    if after.starts_with(',') {
        let comma = text.len() - after.len();
        return format!("{}\n{indent}{key}: {rendered},{}", &text[..=comma], &text[comma + 1..]);
    }
    format!("{},\n{indent}{key}: {rendered}{}", &text[..last.value_end], &text[last.value_end..])
}

/// `value` as pretty JSON, its lines after the first indented by `indent`.
fn render(value: &impl Serialize, indent: &str, unit: &str) -> String {
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(unit.as_bytes());
    let mut serializer = serde_json::Serializer::with_formatter(&mut output, formatter);
    value.serialize(&mut serializer).expect("a JSON value always serializes");
    String::from_utf8(output).expect("JSON is UTF-8").replace('\n', &format!("\n{indent}"))
}

/// The whitespace a line of `text` starts with, for the line holding `position`.
fn line_indent(text: &str, position: usize) -> &str {
    let start = text[..position].rfind('\n').map_or(0, |index| index + 1);
    let line = &text[start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// Whether only whitespace comes before `position` on its line.
fn starts_line(text: &str, position: usize) -> bool {
    let start = text[..position].rfind('\n').map_or(0, |index| index + 1);
    text[start..position].trim_matches([' ', '\t']).is_empty()
}

/// The file's unit of indentation: that of its first indented line, else two spaces.
fn indent_unit(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| &line[..line.len() - line.trim_start_matches([' ', '\t']).len()])
        .find(|indent| !indent.is_empty())
        .map_or_else(|| "  ".to_string(), str::to_string)
}

struct Scanner<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> Scanner<'a> {
    fn at(text: &'a str, position: usize) -> Self {
        Self { text, position }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.position).copied()
    }

    fn error(&self, what: &str) -> String {
        let line = self.text[..self.position.min(self.text.len())].matches('\n').count() + 1;
        format!("{what} on line {line}")
    }

    /// Whitespace and comments.
    fn skip_trivia(&mut self) -> Result<(), String> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.position += 1,
                Some(b'/') if self.text[self.position..].starts_with("//") => {
                    self.position =
                        self.text[self.position..].find('\n').map_or(self.text.len(), |end| self.position + end);
                }
                Some(b'/') if self.text[self.position..].starts_with("/*") => {
                    let end =
                        self.text[self.position + 2..].find("*/").ok_or_else(|| self.error("an unclosed comment"))?;
                    self.position += end + 4;
                }
                _ => return Ok(()),
            }
        }
    }

    /// The object starting here, at its `{`.
    fn object(&mut self) -> Result<Object, String> {
        let open = self.position;
        self.position += 1;
        let mut members = Vec::new();
        loop {
            self.skip_trivia()?;
            match self.peek() {
                Some(b'}') => break,
                Some(b'"') => {}
                _ => return Err(self.error("a key was expected")),
            }
            let key_start = self.position;
            self.string()?;
            let key = serde_json::from_str(&self.text[key_start..self.position]).map_err(|error| error.to_string())?;
            self.skip_trivia()?;
            if self.peek() != Some(b':') {
                return Err(self.error("“:” was expected"));
            }
            self.position += 1;
            self.skip_trivia()?;
            let value_start = self.position;
            self.value()?;
            members.push(Member { key, key_start, value_start, value_end: self.position });
            self.skip_trivia()?;
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => break,
                _ => return Err(self.error("“,” or “}” was expected")),
            }
        }
        let close = self.position;
        self.position += 1;
        Ok(Object { open, close, members })
    }

    fn value(&mut self) -> Result<(), String> {
        match self.peek() {
            Some(b'{') => self.object().map(|_| ()),
            Some(b'[') => {
                self.position += 1;
                loop {
                    self.skip_trivia()?;
                    if self.peek() == Some(b']') {
                        break;
                    }
                    self.value()?;
                    self.skip_trivia()?;
                    match self.peek() {
                        Some(b',') => self.position += 1,
                        Some(b']') => break,
                        _ => return Err(self.error("“,” or “]” was expected")),
                    }
                }
                self.position += 1;
                Ok(())
            }
            Some(b'"') => self.string(),
            _ => {
                let rest = &self.text[self.position..];
                let length = rest.find([',', '}', ']', ' ', '\t', '\r', '\n', '/']).unwrap_or(rest.len());
                if length == 0 {
                    return Err(self.error("a value was expected"));
                }
                self.position += length;
                Ok(())
            }
        }
    }

    fn string(&mut self) -> Result<(), String> {
        let bytes = self.text.as_bytes();
        let mut index = self.position + 1;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index += 2,
                b'"' => {
                    self.position = index + 1;
                    return Ok(());
                }
                _ => index += 1,
            }
        }
        Err(self.error("an unclosed string"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const PATH: [&str; 2] = ["mcpServers", "localhost"];

    #[test]
    fn adds_a_server_beside_the_others_leaving_every_other_byte() {
        let text = "{\n  \"numStartups\": 12,\n  \"mcpServers\": {\n    \"github\": {\n      \"command\": \"gh\"\n    }\n  },\n  \"tipsHistory\": {}\n}\n";
        let edited = set(text, &PATH, &json!({ "args": ["mcp"], "command": "/bin/localhost" })).unwrap();
        assert_eq!(
            edited,
            "{\n  \"numStartups\": 12,\n  \"mcpServers\": {\n    \"github\": {\n      \"command\": \"gh\"\n    },\n    \"localhost\": {\n      \"args\": [\n        \"mcp\"\n      ],\n      \"command\": \"/bin/localhost\"\n    }\n  },\n  \"tipsHistory\": {}\n}\n"
        );
        assert_eq!(remove(&edited, &PATH).unwrap().unwrap(), text);
    }

    #[test]
    fn creates_the_servers_object_and_removes_down_to_an_empty_one() {
        let text = "{\n\t\"theme\": \"dark\"\n}";
        let edited = set(text, &PATH, &json!({ "command": "x" })).unwrap();
        assert_eq!(
            edited,
            "{\n\t\"theme\": \"dark\",\n\t\"mcpServers\": {\n\t\t\"localhost\": {\n\t\t\t\"command\": \"x\"\n\t\t}\n\t}\n}"
        );
        assert_eq!(get(&edited, &PATH).unwrap(), Some(json!({ "command": "x" })));
        assert_eq!(remove(&edited, &PATH).unwrap().unwrap(), "{\n\t\"theme\": \"dark\",\n\t\"mcpServers\": {}\n}");
    }

    #[test]
    fn fills_an_empty_object_or_file() {
        assert_eq!(set("{}", &PATH, &json!(1)).unwrap(), "{\n  \"mcpServers\": {\n    \"localhost\": 1\n  }\n}");
        assert_eq!(
            set("{\n  \"mcpServers\": {}\n}", &PATH, &json!(1)).unwrap(),
            "{\n  \"mcpServers\": {\n    \"localhost\": 1\n  }\n}"
        );
        assert_eq!(set(" \n", &PATH, &json!(1)).unwrap(), "{\n  \"mcpServers\": {\n    \"localhost\": 1\n  }\n}\n");
    }

    #[test]
    fn replaces_only_the_value_it_sets() {
        let text = r#"{"mcpServers": {"localhost": {"command": "old", "env": {"A": "1"}}, "other": {}}}"#;
        let edited = set(text, &["mcpServers", "localhost", "command"], &json!("new")).unwrap();
        assert_eq!(edited, r#"{"mcpServers": {"localhost": {"command": "new", "env": {"A": "1"}}, "other": {}}}"#);
    }

    #[test]
    fn removes_a_first_or_middle_member() {
        let text = "{\n  \"a\": 1,\n  \"b\": [1, {\"c\": \"}\"}],\n  \"d\": 3\n}";
        assert_eq!(remove(text, &["a"]).unwrap().unwrap(), "{\n  \"b\": [1, {\"c\": \"}\"}],\n  \"d\": 3\n}");
        assert_eq!(remove(text, &["b"]).unwrap().unwrap(), "{\n  \"a\": 1,\n  \"d\": 3\n}");
        assert_eq!(remove(text, &["x"]).unwrap(), None);
    }

    #[test]
    fn skips_comments_and_keeps_a_trailing_comma() {
        let text = "// Cursor\n{\n  /* servers */ \"mcpServers\": {\n    \"a\": {}, // first\n  },\n}\n";
        let edited = set(text, &PATH, &json!(true)).unwrap();
        assert_eq!(
            edited,
            "// Cursor\n{\n  /* servers */ \"mcpServers\": {\n    \"a\": {},\n    \"localhost\": true, // first\n  },\n}\n"
        );
        assert_eq!(get(&edited, &PATH).unwrap(), Some(json!(true)));
    }

    #[test]
    fn refuses_what_it_cant_edit() {
        assert!(set("[]", &PATH, &json!(1)).is_err());
        assert!(set("{\"mcpServers\": [1]}", &PATH, &json!(1)).is_err());
        assert!(set("{\"a\": 1", &PATH, &json!(1)).is_err());
        assert!(set("{\"a\": \"unclosed}", &PATH, &json!(1)).is_err());
        assert!(get("{} {}", &PATH).is_err());
    }

    #[test]
    fn reads_escaped_keys() {
        assert_eq!(get(r#"{"mcpServers": {"localhost": 2}}"#, &PATH).unwrap(), Some(json!(2)));
    }
}
