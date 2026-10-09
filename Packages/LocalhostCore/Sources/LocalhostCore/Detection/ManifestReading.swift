import Foundation

/// Just enough TOML to read `pyproject.toml` and `Cargo.toml`: tables,
/// array tables and `key = value` pairs whose values stay raw. Anything it
/// doesn't understand is skipped rather than failing the scan.
struct TOMLDocument {
    struct Table {
        /// e.g. `package`, `tool.poetry`, `bin` for `[[bin]]`.
        var name: String
        var values: [String: String]
    }

    /// In file order; the top level is a table named "".
    var tables: [Table]

    init(_ text: String) {
        var tables = [Table(name: "", values: [:])]
        var pending: (key: String, value: String)?
        for rawLine in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let line = Self.strippingComment(String(rawLine)).trimmingCharacters(in: .whitespaces)
            if let (key, value) = pending {
                let joined = value + " " + line
                if Self.isBalanced(joined) {
                    tables[tables.count - 1].values[key] = joined
                    pending = nil
                } else {
                    pending = (key, joined)
                }
                continue
            }
            guard !line.isEmpty else { continue }
            if line.hasPrefix("[") {
                let name = line.trimmingCharacters(in: CharacterSet(charactersIn: "[] "))
                tables.append(Table(name: name, values: [:]))
                continue
            }
            guard let equals = line.firstIndex(of: "=") else { continue }
            let key = line[..<equals].trimmingCharacters(in: .whitespaces)
                .trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
            let value = line[line.index(after: equals)...].trimmingCharacters(in: .whitespaces)
            if Self.isBalanced(value) {
                tables[tables.count - 1].values[key] = value
            } else {
                pending = (key, value)
            }
        }
        self.tables = tables
    }

    func table(_ name: String) -> Table? {
        tables.first { $0.name == name }
    }

    func tables(named name: String) -> [Table] {
        tables.filter { $0.name == name }
    }

    /// A raw value as a string, if it is a quoted string.
    static func string(_ raw: String?) -> String? {
        guard let raw, raw.count >= 2, let quote = raw.first, quote == "\"" || quote == "'", raw.last == quote
        else { return nil }
        return String(raw.dropFirst().dropLast())
    }

    /// The strings of a raw array value such as `["a", 'b/*']`.
    static func strings(_ raw: String?) -> [String] {
        guard let raw, raw.hasPrefix("[") else { return [] }
        return quotedStrings(in: raw)
    }

    private static func isBalanced(_ value: String) -> Bool {
        value.filter { $0 == "[" }.count <= value.filter { $0 == "]" }.count
    }

    private static func strippingComment(_ line: String) -> String {
        var quote: Character?
        for index in line.indices {
            let character = line[index]
            if let open = quote {
                if character == open { quote = nil }
            } else if character == "\"" || character == "'" {
                quote = character
            } else if character == "#" {
                return String(line[..<index])
            }
        }
        return line
    }
}

/// The quoted strings in a line, e.g. `["a", 'b']` → `a`, `b`.
func quotedStrings(in text: String) -> [String] {
    var strings: [String] = []
    var current: String?
    var quote: Character?
    for character in text {
        if let open = quote {
            if character == open {
                strings.append(current ?? "")
                (current, quote) = (nil, nil)
            } else {
                current = (current ?? "") + String(character)
            }
        } else if character == "\"" || character == "'" {
            (current, quote) = ("", character)
        }
    }
    return strings
}

/// Just enough YAML for workspace files and Compose files: block mappings by
/// indentation, block and flow sequences of scalars. Anchors, multi-line
/// scalars and other features are skipped rather than failing the scan.
struct YAMLLines {
    struct Line {
        var indent: Int
        /// Without indentation, comment and trailing spaces.
        var text: String

        var isListItem: Bool { text == "-" || text.hasPrefix("- ") }

        /// For `key: value` (or `key:`), the unquoted key and the raw value.
        var keyValue: (key: String, value: String)? {
            let body = isListItem ? String(text.dropFirst(2)) : text
            guard let colon = body.range(of: ": ") ?? (body.hasSuffix(":") ? body.range(of: ":", options: .backwards) : nil)
            else { return nil }
            let key = body[..<colon.lowerBound].trimmingCharacters(in: .whitespaces)
            guard !key.isEmpty, !key.contains(" ") || key.first == "\"" || key.first == "'" else { return nil }
            return (YAMLLines.unquoted(key), body[colon.upperBound...].trimmingCharacters(in: .whitespaces))
        }
    }

    var lines: [Line]

    init(_ text: String) {
        lines = text.split(separator: "\n", omittingEmptySubsequences: false).compactMap { raw in
            let raw = raw.replacingOccurrences(of: "\t", with: "  ")
            let indent = raw.prefix { $0 == " " }.count
            let text = Self.strippingComment(String(raw.dropFirst(indent))).trimmingCharacters(in: .whitespaces)
            guard !text.isEmpty, text != "---" else { return nil }
            return Line(indent: indent, text: text)
        }
    }

    /// The lines nested under the top-level `key:`.
    func block(underTopLevelKey key: String) -> ArraySlice<Line> {
        guard let start = lines.firstIndex(where: { $0.indent == 0 && $0.keyValue?.key == key }) else { return [] }
        let rest = lines[(start + 1)...]
        let end = rest.firstIndex { $0.indent == 0 && !$0.isListItem } ?? lines.endIndex
        return lines[(start + 1)..<end]
    }

    /// The scalars of a sequence: either a flow sequence value (`[a, b]`) or
    /// the `- item` lines that follow.
    static func sequence(value: String, following lines: ArraySlice<Line>) -> [String] {
        if value.hasPrefix("[") {
            return value.dropFirst().dropLast().split(separator: ",")
                .map { unquoted($0.trimmingCharacters(in: .whitespaces)) }
                .filter { !$0.isEmpty }
        }
        guard value.isEmpty, let first = lines.first, first.isListItem else { return [] }
        return lines.prefix { $0.indent == first.indent && $0.isListItem }
            .map { unquoted(String($0.text.dropFirst(2)).trimmingCharacters(in: .whitespaces)) }
    }

    static func unquoted(_ text: String) -> String {
        guard text.count >= 2, let quote = text.first, quote == "\"" || quote == "'", text.last == quote
        else { return text }
        return String(text.dropFirst().dropLast())
    }

    private static func strippingComment(_ text: String) -> String {
        if text.hasPrefix("#") { return "" }
        var quote: Character?
        var previous: Character = " "
        for index in text.indices {
            let character = text[index]
            if let open = quote {
                if character == open { quote = nil }
            } else if character == "\"" || character == "'" {
                quote = character
            } else if character == "#", previous == " " {
                return String(text[..<index])
            }
            previous = character
        }
        return text
    }
}

/// Quotes a word for the login shell when it isn't plainly safe.
func shellQuoted(_ word: String) -> String {
    let safe = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789@%+=:,./_-")
    guard word.isEmpty || word.unicodeScalars.contains(where: { !safe.contains($0) }) else { return word }
    return "'" + word.replacingOccurrences(of: "'", with: "'\\''") + "'"
}
