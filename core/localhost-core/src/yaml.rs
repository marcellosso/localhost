//! The YAML localhost reads and writes. There is no YAML dependency: the
//! formats it reads need only a small subset, and Project files are
//! written so that any YAML reader reads them back the same.

use crate::{LocalhostError, ProjectFileProblem};

/// YAML as lines: indentation, then the text without its comment. Detection
/// reads workspace and Compose files from it leniently: block mappings by
/// indentation, block and flow sequences of scalars, skipping anchors,
/// multi-line scalars and the rest rather than failing the scan.
/// [`YamlValue`] reads Project files from it strictly.
pub(crate) struct YamlLines {
    pub lines: Vec<YamlLine>,
}

#[derive(Clone, Debug)]
pub(crate) struct YamlLine {
    /// 1-based, in the original text.
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

/// A YAML value read strictly from [`YamlLines`], with the line it starts on,
/// for Project files. The subset: block mappings, block sequences of
/// scalars, flow sequences of scalars (`[a, b]`), empty flow mappings (`{}`),
/// plain, single- and double-quoted scalars, comments. Anything else (block
/// scalars, anchors, tags, nested flow collections, …) is rejected with its
/// line rather than skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct YamlValue {
    pub line: usize,
    pub kind: YamlKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum YamlKind {
    /// `key:` with nothing after or under it.
    Null,
    Scalar(String),
    Sequence(Vec<YamlValue>),
    /// In file order.
    Mapping(Vec<YamlEntry>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct YamlEntry {
    pub key: String,
    pub line: usize,
    pub value: YamlValue,
}

impl YamlValue {
    /// The document's top-level value; an empty document is `Null`.
    pub fn parse(text: &str) -> Result<YamlValue, LocalhostError> {
        let mut parser = Parser { lines: YamlLines::parse(text).lines, index: 0 };
        parser.document()
    }
}

struct Parser {
    lines: Vec<YamlLine>,
    index: usize,
}

impl Parser {
    fn document(&mut self) -> Result<YamlValue, LocalhostError> {
        let Some(first) = self.lines.first() else { return Ok(YamlValue { line: 1, kind: YamlKind::Null }) };
        let value = self.block(first.indent)?;
        if let Some(line) = self.lines.get(self.index) {
            return Err(error(ProjectFileProblem::UnexpectedIndentation, line.number));
        }
        Ok(value)
    }

    /// The mapping or sequence whose lines are at `indent`.
    fn block(&mut self, indent: usize) -> Result<YamlValue, LocalhostError> {
        let first = self.lines[self.index].clone();
        if first.is_list_item() {
            return self.sequence(indent);
        }
        let mut entries: Vec<YamlEntry> = Vec::new();
        while let Some(line) = self.lines.get(self.index).filter(|line| line.indent == indent).cloned() {
            if line.is_list_item() {
                return Err(error(ProjectFileProblem::ExpectedKeyValue, line.number));
            }
            let (key, rest) = key_value(&line.text, line.number)?;
            if entries.iter().any(|entry| entry.key == key) {
                return Err(LocalhostError::InvalidProjectFile {
                    problem: ProjectFileProblem::Duplicate,
                    field: vec![key],
                    line: Some(line.number),
                });
            }
            self.index += 1;
            let value = self.value(&rest, &line)?;
            entries.push(YamlEntry { key, line: line.number, value });
        }
        Ok(YamlValue { line: first.number, kind: YamlKind::Mapping(entries) })
    }

    fn sequence(&mut self, indent: usize) -> Result<YamlValue, LocalhostError> {
        let first_line = self.lines[self.index].number;
        let mut items = Vec::new();
        while let Some(line) =
            self.lines.get(self.index).filter(|line| line.indent == indent && line.is_list_item()).cloned()
        {
            let rest = line.text[1..].trim().to_string();
            if key_value(&rest, line.number).is_ok() {
                return Err(error(ProjectFileProblem::UnsupportedYaml("lists of mappings".into()), line.number));
            }
            self.index += 1;
            items.push(self.value(&rest, &line)?);
        }
        Ok(YamlValue { line: first_line, kind: YamlKind::Sequence(items) })
    }

    /// The value of `line`, written after its key or dash (`rest`), or on
    /// the lines nested under it.
    fn value(&mut self, rest: &str, line: &YamlLine) -> Result<YamlValue, LocalhostError> {
        let next = self.lines.get(self.index);
        if rest.is_empty() {
            let Some(next) = next else { return Ok(YamlValue { line: line.number, kind: YamlKind::Null }) };
            // A sequence may sit at its key's indentation.
            if next.indent > line.indent || (next.indent == line.indent && next.is_list_item() && !line.is_list_item())
            {
                return self.block(next.indent);
            }
            return Ok(YamlValue { line: line.number, kind: YamlKind::Null });
        }
        let value = inline(rest, line.number)?;
        if let Some(next) = next.filter(|next| next.indent > line.indent) {
            return Err(error(ProjectFileProblem::UnexpectedIndentation, next.number));
        }
        Ok(value)
    }
}

/// A value written on one line: a scalar or a flow collection.
fn inline(text: &str, line: usize) -> Result<YamlValue, LocalhostError> {
    let kind = if let Some(after) = text.strip_prefix('[') {
        let Some(body) = after.strip_suffix(']') else {
            return Err(error(ProjectFileProblem::UnsupportedYaml("flow sequences over several lines".into()), line));
        };
        let mut items = Vec::new();
        if !body.trim().is_empty() {
            for item in split_flow_items(body.trim(), line)? {
                if item.starts_with('[') || item.starts_with('{') {
                    return Err(error(ProjectFileProblem::UnsupportedYaml("nested flow collections".into()), line));
                }
                items.push(YamlValue { line, kind: YamlKind::Scalar(scalar(&item, line)?) });
            }
        }
        YamlKind::Sequence(items)
    } else if let Some(after) = text.strip_prefix('{') {
        if !after.strip_suffix('}').is_some_and(|body| body.trim().is_empty()) {
            return Err(error(ProjectFileProblem::UnsupportedYaml("flow mappings ({…})".into()), line));
        }
        YamlKind::Mapping(Vec::new())
    } else {
        YamlKind::Scalar(scalar(text, line)?)
    };
    Ok(YamlValue { line, kind })
}

/// The text of a plain or quoted scalar.
fn scalar(text: &str, line: usize) -> Result<String, LocalhostError> {
    let unsupported = |feature: &str| Err(error(ProjectFileProblem::UnsupportedYaml(feature.into()), line));
    match text.chars().next() {
        Some('"') => double_quoted(text, line),
        Some('\'') => {
            let body = text.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\''));
            // Inside, a quote is written twice; a lone one would end the scalar early.
            match body {
                Some(body) if !body.replace("''", "").contains('\'') => Ok(body.replace("''", "'")),
                _ => Err(error(ProjectFileProblem::UnterminatedQuote, line)),
            }
        }
        Some('|' | '>') => unsupported("block scalars (| and >)"),
        Some('&' | '*') => unsupported("anchors and aliases"),
        Some('!') => unsupported("tags"),
        _ => Ok(text.to_string()),
    }
}

fn double_quoted(text: &str, line: usize) -> Result<String, LocalhostError> {
    let mut result = String::new();
    let mut chars = text.chars().skip(1);
    let mut is_closed = false;
    while let Some(c) = chars.next() {
        if is_closed {
            return Err(error(ProjectFileProblem::UnterminatedQuote, line));
        }
        match c {
            '"' => is_closed = true,
            '\\' => {
                let escape = chars.next().ok_or_else(|| error(ProjectFileProblem::UnterminatedQuote, line))?;
                match escape {
                    '\\' | '"' | '/' => result.push(escape),
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    'r' => result.push('\r'),
                    '0' => result.push('\0'),
                    ' ' => result.push(' '),
                    'x' | 'u' | 'U' => {
                        let digits = match escape {
                            'x' => 2,
                            'u' => 4,
                            _ => 8,
                        };
                        let hex: String = chars.by_ref().take(digits).collect();
                        let decoded = (hex.chars().count() == digits)
                            .then(|| u32::from_str_radix(&hex, 16).ok())
                            .flatten()
                            .and_then(char::from_u32)
                            .ok_or_else(|| error(ProjectFileProblem::InvalidEscape, line))?;
                        result.push(decoded);
                    }
                    _ => return Err(error(ProjectFileProblem::InvalidEscape, line)),
                }
            }
            _ => result.push(c),
        }
    }
    if !is_closed {
        return Err(error(ProjectFileProblem::UnterminatedQuote, line));
    }
    Ok(result)
}

/// The items of a flow sequence's body, split at commas outside quotes.
fn split_flow_items(body: &str, line: usize) -> Result<Vec<String>, LocalhostError> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut is_escaped = false;
    for c in body.chars() {
        if let Some(open) = quote {
            if is_escaped {
                is_escaped = false;
            } else if open == '"' && c == '\\' {
                is_escaped = true;
            } else if c == open {
                quote = None;
            }
            current.push(c);
        } else if c == ',' {
            items.push(current.trim().to_string());
            current.clear();
        } else {
            if (c == '"' || c == '\'') && current.trim().is_empty() {
                quote = Some(c);
            }
            current.push(c);
        }
    }
    if quote.is_some() {
        return Err(error(ProjectFileProblem::UnterminatedQuote, line));
    }
    items.push(current.trim().to_string());
    // A trailing comma (`[a, b,]`) leaves an empty last item.
    if items.last().is_some_and(String::is_empty) {
        items.pop();
    }
    if items.iter().any(String::is_empty) {
        return Err(error(ProjectFileProblem::ExpectedKeyValue, line));
    }
    Ok(items)
}

/// Splits `key: value` (or `key:`) into the key and the rest; a quoted key
/// may contain `: `.
fn key_value(text: &str, line: usize) -> Result<(String, String), LocalhostError> {
    let key_end = match text.chars().next() {
        Some(quote @ ('"' | '\'')) => {
            let mut is_escaped = false;
            let mut closing = None;
            let mut chars = text.char_indices().skip(1).peekable();
            while let Some((index, c)) = chars.next() {
                if is_escaped {
                    is_escaped = false;
                } else if quote == '"' && c == '\\' {
                    is_escaped = true;
                } else if c == quote {
                    // `''` inside single quotes is an escaped quote.
                    if quote == '\'' && chars.peek().is_some_and(|&(_, next)| next == '\'') {
                        chars.next();
                    } else {
                        closing = Some(index);
                        break;
                    }
                }
            }
            let closing = closing.ok_or_else(|| error(ProjectFileProblem::UnterminatedQuote, line))?;
            if !text[closing + 1..].starts_with(':') {
                return Err(error(ProjectFileProblem::ExpectedKeyValue, line));
            }
            closing + 1
        }
        _ => match text.find(": ") {
            Some(colon) => colon,
            None if text.ends_with(':') => text.len() - 1,
            None => return Err(error(ProjectFileProblem::ExpectedKeyValue, line)),
        },
    };
    let raw_key = text[..key_end].trim();
    if raw_key.is_empty() {
        return Err(error(ProjectFileProblem::ExpectedKeyValue, line));
    }
    if raw_key.starts_with(['[', '{', '?']) {
        return Err(error(ProjectFileProblem::UnsupportedYaml("complex keys".into()), line));
    }
    Ok((scalar(raw_key, line)?, text[key_end + 1..].trim().to_string()))
}

fn error(problem: ProjectFileProblem, line: usize) -> LocalhostError {
    LocalhostError::InvalidProjectFile { problem, field: Vec::new(), line: Some(line) }
}

/// Writes `text` as a scalar that [`YamlValue`] (and any YAML reader) reads
/// back as the same text: plain when that is unambiguous, otherwise
/// double-quoted.
pub(crate) fn write_scalar(text: &str) -> String {
    if is_plain_safe(text) { text.to_string() } else { double_quoted_scalar(text) }
}

fn double_quoted_scalar(text: &str) -> String {
    let mut result = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            '\r' => result.push_str("\\r"),
            c if is_control(c) => result.push_str(&format!("\\x{:02X}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn is_control(c: char) -> bool {
    (c as u32) < 0x20 || c == '\u{7F}'
}

/// Whether `text` reads back as itself, as a string, without quotes.
fn is_plain_safe(text: &str) -> bool {
    let (Some(first), Some(last)) = (text.chars().next(), text.chars().last()) else { return false };
    !first.is_whitespace()
        && !last.is_whitespace()
        && last != ':'
        && !"-?:,[]{}#&*!|>'\"%@`".contains(first)
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.chars().any(is_control)
        && !looks_like_non_string(text)
}

/// Null, booleans and numbers, as YAML 1.1 or 1.2 readers would take them.
fn looks_like_non_string(text: &str) -> bool {
    const WORDS: [&str; 14] =
        ["~", "null", "true", "false", "yes", "no", "on", "off", "y", "n", ".inf", "-.inf", "+.inf", ".nan"];
    if WORDS.contains(&text.to_lowercase().as_str()) || text.parse::<f64>().is_ok() {
        return true;
    }
    let Some(first) = text.chars().next() else { return false };
    // Integers with prefixes, underscores or sexagesimal parts (`0x1F`, `1_000`, `12:30`).
    let starts_numeric = first.is_ascii_digit() || ((first == '+' || first == '.') && text.chars().count() > 1);
    starts_numeric && text.chars().all(|c| c.is_ascii_hexdigit() || "xXoObB_:.+-".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_comments_stop_at_a_quote_only_where_a_scalar_starts() {
        let lines = YamlLines::parse("a: \"x # y\" # gone\nb: it's # gone\n# whole line\nc: 'q' # gone\n");
        let texts: Vec<&str> = lines.lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts, ["a: \"x # y\"", "b: it's", "c: 'q'"]);
    }

    #[test]
    fn writes_values_that_read_back_as_the_same_text() {
        let values = [
            "",
            " leading",
            "trailing ",
            "true",
            "No",
            "null",
            "~",
            "3000",
            "1.5",
            "0x1F",
            "12:30",
            "-v",
            "[a]",
            "{}",
            "a: b",
            "a #b",
            "a#b",
            "ends:",
            "say \"hi\"",
            "it's",
            "back\\slash",
            "line\nbreak",
            "tab\there",
            "bell\u{7}",
            "ünïcødé ✓",
            "$PORT",
            "http://localhost:3000/a?b=c&d#e",
            "'quoted'",
            "\"quoted\"",
            "%x",
            "@x",
            "* x",
            "! x",
        ];
        for value in values {
            let yaml = format!("localhost: 1\nkey: {}\n", write_scalar(value));
            let YamlKind::Mapping(entries) = YamlValue::parse(&yaml).unwrap().kind else { panic!("Not a mapping") };
            assert_eq!(entries[1].value.kind, YamlKind::Scalar(value.to_string()), "{value:?}");
        }
    }

    #[test]
    fn writes_plain_only_what_reads_back_as_text() {
        assert_eq!(write_scalar("pnpm dev"), "pnpm dev");
        assert_eq!(write_scalar("$PORT"), "$PORT");
        assert_eq!(write_scalar("3000"), "\"3000\"");
        assert_eq!(write_scalar("yes"), "\"yes\"");
        assert_eq!(write_scalar("bell\u{7}"), "\"bell\\x07\"");
    }

    #[test]
    fn reads_quoted_keys_escapes_and_flow_sequences() {
        let yaml = "\"a: b\": 1\n'it''s': [x, 'y, z', \"\\u00e9\",]\nempty: []\nnone: {}\n";
        let YamlKind::Mapping(entries) = YamlValue::parse(yaml).unwrap().kind else { panic!("Not a mapping") };
        let keys: Vec<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
        assert_eq!(keys, ["a: b", "it's", "empty", "none"]);
        let scalars = |value: &YamlValue| match &value.kind {
            YamlKind::Sequence(items) => items
                .iter()
                .map(|item| match &item.kind {
                    YamlKind::Scalar(text) => text.clone(),
                    other => panic!("{other:?}"),
                })
                .collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        assert_eq!(scalars(&entries[1].value), ["x", "y, z", "é"]);
        assert!(scalars(&entries[2].value).is_empty());
        assert_eq!(entries[3].value.kind, YamlKind::Mapping(Vec::new()));
    }

    #[test]
    fn rejects_what_it_does_not_support_with_its_line() {
        let problem = |yaml: &str| match YamlValue::parse(yaml) {
            Err(LocalhostError::InvalidProjectFile { problem, line, .. }) => (problem, line),
            other => panic!("{yaml:?}: {other:?}"),
        };
        let unsupported = |feature: &str| ProjectFileProblem::UnsupportedYaml(feature.into());
        assert_eq!(problem("a:\n  - b: c\n"), (unsupported("lists of mappings"), Some(2)));
        assert_eq!(problem("a: &x b\n"), (unsupported("anchors and aliases"), Some(1)));
        assert_eq!(problem("a: !tag b\n"), (unsupported("tags"), Some(1)));
        assert_eq!(problem("a: [b, [c]]\n"), (unsupported("nested flow collections"), Some(1)));
        assert_eq!(problem("a: [b,\n  c]\n"), (unsupported("flow sequences over several lines"), Some(1)));
        assert_eq!(problem("a: {b: c}\n"), (unsupported("flow mappings ({…})"), Some(1)));
        assert_eq!(problem("? a\n"), (ProjectFileProblem::ExpectedKeyValue, Some(1)));
        assert_eq!(problem("a: \"\\q\"\n"), (ProjectFileProblem::InvalidEscape, Some(1)));
        assert_eq!(problem("a: 'it's'\n"), (ProjectFileProblem::UnterminatedQuote, Some(1)));
        assert_eq!(problem("a: b\n  c: d\n"), (ProjectFileProblem::UnexpectedIndentation, Some(2)));
    }
}
