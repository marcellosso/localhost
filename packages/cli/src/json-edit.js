// Edits a JSON configuration file in place: one member set or removed, every
// other character as it was. Agents' files hold far more than the MCP servers
// (`~/.claude.json` is Claude Code's whole state), so they are never parsed
// and written back whole, which would reorder and reformat them. Comments
// and trailing commas, which Cursor tolerates, are skipped like whitespace.
// The same editor as the core's (core/localhost-core/src/agents/json_edit.rs).

/** The value at `path`, e.g. `["mcpServers", "localhost"]`, or `undefined`. Throws a reason when the file can't be read. */
export function get(text, path) {
  if (text.trim() === '') return undefined;
  let object = root(text);
  for (const [index, key] of path.entries()) {
    const member = object.members.find((candidate) => candidate.key === key);
    if (!member) return undefined;
    const slice = text.slice(member.valueStart, member.valueEnd);
    if (index === path.length - 1) {
      try {
        return JSON.parse(slice);
      } catch (error) {
        throw reason(error.message);
      }
    }
    if (!slice.startsWith('{')) return undefined;
    object = new Scanner(text, member.valueStart).object();
  }
  return undefined;
}

/** `text` with the value at `path` set to `value`, creating the objects on the way. Throws when something on the way isn't an object. */
export function set(text, path, value) {
  const unit = indentUnit(text);
  if (text.trim() === '') return `${renderNested(path, value, '', unit)}\n`;
  let object = root(text);
  for (const [index, key] of path.entries()) {
    const member = object.members.find((candidate) => candidate.key === key);
    if (!member) return insert(text, object, key, path.slice(index + 1), value, unit);
    if (index === path.length - 1) {
      const indent = lineIndent(text, member.keyStart);
      return text.slice(0, member.valueStart) + render(value, indent, unit) + text.slice(member.valueEnd);
    }
    if (!text.startsWith('{', member.valueStart)) throw reason(`“${key}” isn’t an object`);
    object = new Scanner(text, member.valueStart).object();
  }
  throw new Error('the path is never empty');
}

/** `text` without the member at `path`, or `undefined` when there is none. */
export function remove(text, path) {
  const key = path.at(-1);
  if (key === undefined || text.trim() === '') return undefined;
  let object = root(text);
  for (const parent of path.slice(0, -1)) {
    const member = object.members.find((candidate) => candidate.key === parent);
    if (!member || !text.startsWith('{', member.valueStart)) return undefined;
    object = new Scanner(text, member.valueStart).object();
  }
  const index = object.members.findIndex((member) => member.key === key);
  if (index === -1) return undefined;
  const member = object.members[index];
  let start;
  let end;
  if (object.members.length === 1) {
    [start, end] = [object.open + 1, object.close];
  } else if (index + 1 < object.members.length) {
    // Up to the next key: its line keeps this member's indentation.
    [start, end] = [member.keyStart, object.members[index + 1].keyStart];
  } else {
    // The last one: from the end of the one before, with its comma.
    [start, end] = [object.members[index - 1].valueEnd, member.valueEnd];
  }
  return text.slice(0, start) + text.slice(end);
}

/** An error whose message says why a file can't be edited. */
function reason(message) {
  const error = new Error(message);
  error.reason = true;
  return error;
}

function root(text) {
  const scanner = new Scanner(text, 0);
  scanner.skipTrivia();
  if (scanner.peek() !== '{') throw reason('it isn’t a JSON object');
  const object = scanner.object();
  scanner.skipTrivia();
  if (scanner.peek() !== undefined) throw reason('there is more after the JSON object');
  return object;
}

/** `{ path[0]: { path[1]: … value } }` rendered, or `value` itself for an empty path. */
function renderNested(path, value, indent, unit) {
  if (path.length === 0) return render(value, indent, unit);
  const innerIndent = indent + unit;
  const inner = renderNested(path.slice(1), value, innerIndent, unit);
  return `{\n${innerIndent}${JSON.stringify(path[0])}: ${inner}\n${indent}}`;
}

/** `text` with `"key": value` added as the last member of `object` (inside new objects for each of `path`), indented like its other members. */
function insert(text, object, key, path, value, unit) {
  const parentIndent = lineIndent(text, object.open);
  const quoted = JSON.stringify(key);
  const last = object.members.at(-1);
  if (!last) {
    const indent = parentIndent + unit;
    const rendered = renderNested(path, value, indent, unit);
    return `${text.slice(0, object.open + 1)}\n${indent}${quoted}: ${rendered}\n${parentIndent}${text.slice(object.close)}`;
  }
  const indent = startsLine(text, last.keyStart) ? lineIndent(text, last.keyStart) : parentIndent + unit;
  const rendered = renderNested(path, value, indent, unit);
  // A trailing comma (JSONC) stays the last thing before the brace.
  const after = text.slice(last.valueEnd).replace(/^[ \t\r\n]+/, '');
  if (after.startsWith(',')) {
    const comma = text.length - after.length;
    return `${text.slice(0, comma + 1)}\n${indent}${quoted}: ${rendered},${text.slice(comma + 1)}`;
  }
  return `${text.slice(0, last.valueEnd)},\n${indent}${quoted}: ${rendered}${text.slice(last.valueEnd)}`;
}

/** `value` as pretty JSON, its lines after the first indented by `indent`. */
function render(value, indent, unit) {
  return JSON.stringify(value, null, unit).replaceAll('\n', `\n${indent}`);
}

/** The whitespace the line holding `position` starts with. */
function lineIndent(text, position) {
  const start = text.lastIndexOf('\n', position - 1) + 1;
  return /^[ \t]*/.exec(text.slice(start))[0];
}

/** Whether only whitespace comes before `position` on its line. */
function startsLine(text, position) {
  const start = text.lastIndexOf('\n', position - 1) + 1;
  return text.slice(start, position).trim() === '';
}

/** The file's unit of indentation: that of its first indented line, else two spaces. */
function indentUnit(text) {
  for (const line of text.split('\n')) {
    if (line.trim() === '') continue;
    const indent = /^[ \t]*/.exec(line)[0];
    if (indent !== '') return indent;
  }
  return '  ';
}

class Scanner {
  constructor(text, position) {
    this.text = text;
    this.position = position;
  }

  peek() {
    return this.text[this.position];
  }

  error(what) {
    const line = this.text.slice(0, this.position).split('\n').length;
    return reason(`${what} on line ${line}`);
  }

  /** Whitespace and comments. */
  skipTrivia() {
    for (;;) {
      const character = this.peek();
      if (character === ' ' || character === '\t' || character === '\r' || character === '\n') {
        this.position += 1;
      } else if (this.text.startsWith('//', this.position)) {
        const end = this.text.indexOf('\n', this.position);
        this.position = end === -1 ? this.text.length : end;
      } else if (this.text.startsWith('/*', this.position)) {
        const end = this.text.indexOf('*/', this.position + 2);
        if (end === -1) throw this.error('an unclosed comment');
        this.position = end + 2;
      } else {
        return;
      }
    }
  }

  /** The object starting here, at its `{`. */
  object() {
    const open = this.position;
    this.position += 1;
    const members = [];
    for (;;) {
      this.skipTrivia();
      if (this.peek() === '}') break;
      if (this.peek() !== '"') throw this.error('a key was expected');
      const keyStart = this.position;
      this.string();
      let key;
      try {
        key = JSON.parse(this.text.slice(keyStart, this.position));
      } catch (error) {
        throw reason(error.message);
      }
      this.skipTrivia();
      if (this.peek() !== ':') throw this.error('“:” was expected');
      this.position += 1;
      this.skipTrivia();
      const valueStart = this.position;
      this.value();
      members.push({ key, keyStart, valueStart, valueEnd: this.position });
      this.skipTrivia();
      if (this.peek() === ',') this.position += 1;
      else if (this.peek() === '}') break;
      else throw this.error('“,” or “}” was expected');
    }
    const close = this.position;
    this.position += 1;
    return { open, close, members };
  }

  value() {
    const character = this.peek();
    if (character === '{') {
      this.object();
    } else if (character === '[') {
      this.position += 1;
      for (;;) {
        this.skipTrivia();
        if (this.peek() === ']') break;
        this.value();
        this.skipTrivia();
        if (this.peek() === ',') this.position += 1;
        else if (this.peek() === ']') break;
        else throw this.error('“,” or “]” was expected');
      }
      this.position += 1;
    } else if (character === '"') {
      this.string();
    } else {
      const length = this.text.slice(this.position).search(/[,}\] \t\r\n\/]/);
      const end = length === -1 ? this.text.length - this.position : length;
      if (end === 0) throw this.error('a value was expected');
      this.position += end;
    }
  }

  string() {
    let index = this.position + 1;
    while (index < this.text.length) {
      if (this.text[index] === '\\') index += 2;
      else if (this.text[index] === '"') {
        this.position = index + 1;
        return;
      } else index += 1;
    }
    throw this.error('an unclosed string');
  }
}
