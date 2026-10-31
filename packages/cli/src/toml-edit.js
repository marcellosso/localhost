// Edits one table of a TOML configuration file (Codex's `config.toml`) in
// place, every other line as it was. It knows only what that takes: where
// tables and keys start, outside strings and multi-line arrays, and the
// values of a server's `command` and `args`. The same editor as the core's
// (core/localhost-core/src/agents/toml_edit.rs).

/** An error whose message says why a file can't be edited. */
function reason(message) {
  const error = new Error(message);
  error.reason = true;
  return error;
}

/**
 * The table at `path` (e.g. `["mcp_servers", "localhost"]`), written as `[path]`, as `{ string(key), strings(key) }`; `undefined` when
 * there is none. Throws when the file can't be read, or defines the table another way (inline, or with dotted keys).
 */
export function table(text, path) {
  const all = lines(text);
  checkWrittenAsTables(all, path);
  const header = all.findIndex((line) => line.kind === 'header' && is(line.path, path));
  if (header === -1) return undefined;
  const entries = [];
  for (const line of all.slice(header + 1)) {
    if (line.kind === 'header') break;
    if (line.kind === 'key' && line.path.length === 1) {
      entries.push([line.path[0], text.slice(line.valueStart, all[line.endLine].end).trim()]);
    }
  }
  const raw = (key) => entries.find(([name]) => name === key)?.[1];
  const parse = (key, read) => {
    const value = raw(key);
    if (value === undefined) return undefined;
    const parser = new ValueParser(value);
    const parsed = read(parser);
    return parsed !== undefined && parser.end() ? parsed : undefined;
  };
  return {
    string: (key) => parse(key, (parser) => parser.string()),
    strings: (key) => parse(key, (parser) => parser.strings()),
  };
}

/**
 * `text` with the table at `path` holding `entries` (`[key, rendered value]`): each one replaced where it is, or added under the
 * header; the table added at the end if there is none. Its other keys stay.
 */
export function set(text, path, entries) {
  const all = lines(text);
  checkWrittenAsTables(all, path);
  const header = all.findIndex((line) => line.kind === 'header' && is(line.path, path));
  if (header === -1) {
    let output = text;
    if (output !== '' && !output.endsWith('\n')) output += '\n';
    if (output.trim() !== '' && !output.endsWith('\n\n')) output += '\n';
    output += `[${renderPath(path)}]\n`;
    for (const [key, value] of entries) output += `${key} = ${value}\n`;
    return output;
  }
  const next = all.slice(header + 1).findIndex((line) => line.kind === 'header');
  const end = next === -1 ? all.length : header + 1 + next;
  const replacements = [];
  const missing = [];
  for (const [key, value] of entries) {
    let found;
    for (let index = header + 1; index < end; index += 1) {
      const line = all[index];
      if (line.kind === 'key' && line.path.length === 1 && line.path[0] === key) {
        found = { index, valueStart: line.valueStart, endLine: line.endLine };
        break;
      }
    }
    if (!found) {
      missing.push(`${key} = ${value}\n`);
      continue;
    }
    const rest = text.slice(found.valueStart, all[found.endLine].end);
    const newline = rest.endsWith('\n') ? '\n' : '';
    // A comment after a one-line value stays; one inside a multi-line array goes with it.
    const comment = found.endLine === found.index ? rest.slice(stripComment(rest).length).replace(/[\r\n]+$/, '') : '';
    const spacing = comment === '' ? '' : ' ';
    replacements.push([found.valueStart, all[found.endLine].end, `${value}${spacing}${comment}${newline}`]);
  }
  if (missing.length > 0) {
    const at = all[header].end;
    const prefix = text.slice(0, at).endsWith('\n') ? '' : '\n';
    replacements.push([at, at, prefix + missing.join('')]);
  }
  // Edits from the bottom up, so earlier offsets stay right.
  replacements.sort((a, b) => b[0] - a[0]);
  let output = text;
  for (const [start, stop, replacement] of replacements) output = output.slice(0, start) + replacement + output.slice(stop);
  return output;
}

/** `text` without the table at `path` and its sub-tables (`[path.env]`), or `undefined` when there is none. */
export function remove(text, path) {
  const all = lines(text);
  checkWrittenAsTables(all, path);
  const ranges = [];
  let index = 0;
  while (index < all.length) {
    if (all[index].kind === 'header' && startsWith(all[index].path, path)) {
      const next = all.slice(index + 1).findIndex((line) => line.kind === 'header');
      const end = next === -1 ? all.length : index + 1 + next;
      ranges.push([all[index].start, end < all.length ? all[end].start : text.length]);
      index = end;
    } else {
      index += 1;
    }
  }
  if (ranges.length === 0) return undefined;
  let output = text;
  for (const [start, end] of ranges.reverse()) output = output.slice(0, start) + output.slice(end);
  // The blank line that separated it from the table before, at the end.
  if (output.endsWith('\n\n') && !text.endsWith('\n\n')) output = output.slice(0, -1);
  return output;
}

/** A TOML basic string. */
export function renderString(value) {
  let output = '"';
  for (const character of value) {
    const code = character.codePointAt(0);
    if (character === '"') output += '\\"';
    else if (character === '\\') output += '\\\\';
    else if (character === '\n') output += '\\n';
    else if (character === '\t') output += '\\t';
    else if (character === '\r') output += '\\r';
    else if (code < 0x20 || (code >= 0x7f && code <= 0x9f)) output += `\\u${code.toString(16).toUpperCase().padStart(4, '0')}`;
    else output += character;
  }
  return `${output}"`;
}

export function renderStrings(values) {
  return `[${values.map(renderString).join(', ')}]`;
}

function renderPath(path) {
  return path.map((key) => (/^[A-Za-z0-9_-]+$/.test(key) ? key : renderString(key))).join('.');
}

function startsWith(header, path) {
  return header.length >= path.length && path.every((part, index) => header[index] === part);
}

function is(header, path) {
  return header.length === path.length && startsWith(header, path);
}

/** Throws when a key outside `[path]` tables reaches into `path`: an inline table (`localhost = { … }` under `[mcp_servers]`), or dotted keys. */
function checkWrittenAsTables(all, path) {
  let current = [];
  for (const line of all) {
    if (line.kind === 'header') {
      current = line.path;
    } else if (line.kind === 'key') {
      const full = [...current, ...line.path];
      const overlap = Math.min(full.length, path.length);
      if (!startsWith(current, path) && full.slice(0, overlap).every((part, index) => part === path[index])) {
        throw reason(`it sets “${full.join('.')}” another way than as a [${path.join('.')}] table`);
      }
    }
  }
}

/** `value` up to a `#` comment outside strings, without the line break. */
function stripComment(value) {
  let quote;
  let escaped = false;
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (quote === '"' && escaped) escaped = false;
    else if (quote === '"' && character === '\\') escaped = true;
    else if (quote !== undefined && character === quote) quote = undefined;
    else if (quote !== undefined) continue;
    else if (character === '"' || character === "'") quote = character;
    else if (character === '#') return value.slice(0, index);
  }
  return value.replace(/[\r\n]+$/, '');
}

/** Splits `text` into lines (`{ start, end, kind }`), finding headers and keys. */
function lines(text) {
  const all = [];
  const state = new State();
  let openKey;
  let start = 0;
  while (start < text.length) {
    const newline = text.indexOf('\n', start);
    const end = newline === -1 ? text.length : newline + 1;
    const line = text.slice(start, end);
    const number = all.length;
    let entry;
    if (state.isClean()) {
      const trimmed = line.trimStart();
      const indent = line.length - trimmed.length;
      if (trimmed.startsWith('[')) {
        const path = header(trimmed);
        if (!path) throw reason(`line ${number + 1} isn’t a table header TOML can read`);
        entry = { kind: 'header', path };
      } else if (trimmed === '' || trimmed.startsWith('#')) {
        entry = { kind: 'other' };
      } else {
        const parsed = key(trimmed);
        if (!parsed) throw reason(`line ${number + 1} isn’t a key TOML can read`);
        const [path, after] = parsed;
        const valueStart = start + indent + (trimmed.length - after.length);
        state.scan(after);
        if (!state.isClean()) openKey = number;
        entry = { kind: 'key', path, valueStart, endLine: number };
      }
    } else {
      state.scan(line);
      if (state.isClean() && openKey !== undefined) {
        all[openKey].endLine = number;
        openKey = undefined;
      }
      entry = { kind: 'other' };
    }
    all.push({ start, end, ...entry });
    start = end;
  }
  if (!state.isClean()) throw reason('a string or an array isn’t closed');
  return all;
}

/** Where a scan is between lines: inside a multi-line string, or how deep in brackets. */
class State {
  multiline = undefined;
  depth = 0;

  isClean() {
    return this.multiline === undefined && this.depth === 0;
  }

  /** Follows strings, comments and brackets to the end of `line`. */
  scan(line) {
    let rest = line;
    while (rest !== '') {
      if (this.multiline !== undefined) {
        const index = rest.indexOf(this.multiline);
        if (index === -1) return;
        if (this.multiline === '"""' && isEscaped(rest.slice(0, index))) {
          rest = rest.slice(index + 1);
        } else {
          this.multiline = undefined;
          rest = rest.slice(index + 3);
        }
        continue;
      }
      if (rest.startsWith('"""') || rest.startsWith("'''")) {
        this.multiline = rest.slice(0, 3);
        rest = rest.slice(3);
      } else if (rest[0] === '"' || rest[0] === "'") {
        const quote = rest[0];
        let index = 1;
        while (index < rest.length && rest[index] !== quote) index += quote === '"' && rest[index] === '\\' ? 2 : 1;
        rest = rest.slice(Math.min(index + 1, rest.length));
      } else if (rest[0] === '#') {
        return;
      } else {
        if (rest[0] === '[' || rest[0] === '{') this.depth += 1;
        else if ((rest[0] === ']' || rest[0] === '}') && this.depth > 0) this.depth -= 1;
        rest = rest.slice(1);
      }
    }
  }
}

/** Whether the text before a `"""` ends in an odd number of backslashes. */
function isEscaped(before) {
  return /\\*$/.exec(before)[0].length % 2 === 1;
}

/** `[a."b c".d]` → `["a", "b c", "d"]`; `[[a]]` too. */
function header(line) {
  const double = line.startsWith('[[');
  const parsed = dottedKey(line.slice(double ? 2 : 1));
  if (!parsed) return undefined;
  const [path, after] = parsed;
  let rest = after.trimStart();
  const close = double ? ']]' : ']';
  if (!rest.startsWith(close)) return undefined;
  rest = rest.slice(close.length).trim();
  return rest === '' || rest.startsWith('#') ? path : undefined;
}

/** `a.b = value` → `[["a", "b"], "value…"]`. */
function key(line) {
  const parsed = dottedKey(line);
  if (!parsed) return undefined;
  const rest = parsed[1].trimStart();
  if (!rest.startsWith('=')) return undefined;
  return [parsed[0], rest.slice(1).replace(/^[ \t]+/, '')];
}

function dottedKey(text) {
  const path = [];
  let rest = text.replace(/^[ \t]+/, '');
  for (;;) {
    let part;
    let after;
    if (rest.startsWith('"') || rest.startsWith("'")) {
      const parser = new ValueParser(rest);
      part = parser.string();
      if (part === undefined) return undefined;
      after = rest.slice(parser.position);
    } else {
      const length = /^[A-Za-z0-9_-]*/.exec(rest)[0].length;
      if (length === 0) return undefined;
      part = rest.slice(0, length);
      after = rest.slice(length);
    }
    path.push(part);
    after = after.replace(/^[ \t]+/, '');
    if (!after.startsWith('.')) return [path, after];
    rest = after.slice(1).replace(/^[ \t]+/, '');
  }
}

/** Reads the few TOML values a server entry needs: strings and arrays of them. */
class ValueParser {
  constructor(text) {
    this.text = text;
    this.position = 0;
  }

  rest() {
    return this.text.slice(this.position);
  }

  skipSpace() {
    for (;;) {
      const rest = this.rest();
      const trimmed = rest.trimStart();
      this.position += rest.length - trimmed.length;
      if (!trimmed.startsWith('#')) return;
      const end = trimmed.indexOf('\n');
      this.position += end === -1 ? trimmed.length : end;
    }
  }

  end() {
    this.skipSpace();
    return this.rest() === '';
  }

  string() {
    this.skipSpace();
    const rest = this.rest();
    if (rest.startsWith("'")) {
      const end = rest.indexOf("'", 1);
      if (end === -1) return undefined;
      this.position += end + 1;
      return rest.slice(1, end);
    }
    if (!rest.startsWith('"')) return undefined;
    let value = '';
    let index = 1;
    while (index < rest.length) {
      const character = rest[index];
      if (character === '"') {
        this.position += index + 1;
        return value;
      }
      if (character !== '\\') {
        value += character;
        index += 1;
        continue;
      }
      const escape = rest[index + 1];
      const simple = { n: '\n', t: '\t', r: '\r', b: '\b', f: '\f', '"': '"', '\\': '\\' }[escape];
      if (simple !== undefined) {
        value += simple;
        index += 2;
      } else if (escape === 'u' || escape === 'U') {
        const length = escape === 'u' ? 4 : 8;
        const hex = rest.slice(index + 2, index + 2 + length);
        if (!/^[0-9A-Fa-f]+$/.test(hex) || hex.length !== length) return undefined;
        const code = Number.parseInt(hex, 16);
        if (code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return undefined;
        value += String.fromCodePoint(code);
        index += 2 + length;
      } else {
        return undefined;
      }
    }
    return undefined;
  }

  strings() {
    this.skipSpace();
    if (!this.rest().startsWith('[')) return undefined;
    this.position += 1;
    const values = [];
    for (;;) {
      this.skipSpace();
      if (this.rest().startsWith(']')) {
        this.position += 1;
        return values;
      }
      const value = this.string();
      if (value === undefined) return undefined;
      values.push(value);
      this.skipSpace();
      if (this.rest().startsWith(',')) this.position += 1;
      else if (!this.rest().startsWith(']')) return undefined;
    }
  }
}
