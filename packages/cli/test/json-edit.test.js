// The core's json_edit tests, against the same editor in JavaScript.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { get, remove, set } from '../src/json-edit.js';

const PATH = ['mcpServers', 'localhost'];

test('adds a server beside the others, leaving every other character', () => {
  const text = '{\n  "numStartups": 12,\n  "mcpServers": {\n    "github": {\n      "command": "gh"\n    }\n  },\n  "tipsHistory": {}\n}\n';
  const edited = set(text, PATH, { args: ['mcp'], command: '/bin/localhost' });
  assert.equal(
    edited,
    '{\n  "numStartups": 12,\n  "mcpServers": {\n    "github": {\n      "command": "gh"\n    },\n    "localhost": {\n      "args": [\n        "mcp"\n      ],\n      "command": "/bin/localhost"\n    }\n  },\n  "tipsHistory": {}\n}\n',
  );
  assert.equal(remove(edited, PATH), text);
});

test('creates the servers object and removes down to an empty one', () => {
  const text = '{\n\t"theme": "dark"\n}';
  const edited = set(text, PATH, { command: 'x' });
  assert.equal(edited, '{\n\t"theme": "dark",\n\t"mcpServers": {\n\t\t"localhost": {\n\t\t\t"command": "x"\n\t\t}\n\t}\n}');
  assert.deepEqual(get(edited, PATH), { command: 'x' });
  assert.equal(remove(edited, PATH), '{\n\t"theme": "dark",\n\t"mcpServers": {}\n}');
});

test('fills an empty object or file', () => {
  assert.equal(set('{}', PATH, 1), '{\n  "mcpServers": {\n    "localhost": 1\n  }\n}');
  assert.equal(set('{\n  "mcpServers": {}\n}', PATH, 1), '{\n  "mcpServers": {\n    "localhost": 1\n  }\n}');
  assert.equal(set(' \n', PATH, 1), '{\n  "mcpServers": {\n    "localhost": 1\n  }\n}\n');
});

test('replaces only the value it sets', () => {
  const text = '{"mcpServers": {"localhost": {"command": "old", "env": {"A": "1"}}, "other": {}}}';
  assert.equal(set(text, ['mcpServers', 'localhost', 'command'], 'new'), '{"mcpServers": {"localhost": {"command": "new", "env": {"A": "1"}}, "other": {}}}');
});

test('removes a first or middle member', () => {
  const text = '{\n  "a": 1,\n  "b": [1, {"c": "}"}],\n  "d": 3\n}';
  assert.equal(remove(text, ['a']), '{\n  "b": [1, {"c": "}"}],\n  "d": 3\n}');
  assert.equal(remove(text, ['b']), '{\n  "a": 1,\n  "d": 3\n}');
  assert.equal(remove(text, ['x']), undefined);
});

test('skips comments and keeps a trailing comma', () => {
  const text = '// Cursor\n{\n  /* servers */ "mcpServers": {\n    "a": {}, // first\n  },\n}\n';
  const edited = set(text, PATH, true);
  assert.equal(edited, '// Cursor\n{\n  /* servers */ "mcpServers": {\n    "a": {},\n    "localhost": true, // first\n  },\n}\n');
  assert.equal(get(edited, PATH), true);
});

test('refuses what it can’t edit', () => {
  assert.throws(() => set('[]', PATH, 1), /isn’t a JSON object/);
  assert.throws(() => set('{"mcpServers": [1]}', PATH, 1), /isn’t an object/);
  assert.throws(() => set('{"a": 1', PATH, 1));
  assert.throws(() => set('{"a": "unclosed}', PATH, 1), /unclosed string/);
  assert.throws(() => get('{} {}', PATH), /more after/);
});

test('reads escaped keys and strings', () => {
  assert.equal(get('{"mcpServers": {"local\\u0068ost": 2}}', PATH), 2);
  assert.deepEqual(get('{"mcpServers": {"localhost": {"command": "C:\\\\a \\"b\\""}}}', PATH), { command: 'C:\\a "b"' });
});
