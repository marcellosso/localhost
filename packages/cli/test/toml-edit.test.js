// The core's toml_edit tests, against the same editor in JavaScript.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { remove, renderString, renderStrings, set, table } from '../src/toml-edit.js';

const PATH = ['mcp_servers', 'localhost'];
const entries = () => [['command', renderString('/opt/local host/localhost')], ['args', renderStrings(['mcp'])]];

test('adds a table at the end and removes it again', () => {
  const text = 'model = "gpt-5"\n\n[mcp_servers.github]\ncommand = "gh"\n';
  const edited = set(text, PATH, entries());
  assert.equal(edited, 'model = "gpt-5"\n\n[mcp_servers.github]\ncommand = "gh"\n\n[mcp_servers.localhost]\ncommand = "/opt/local host/localhost"\nargs = ["mcp"]\n');
  const found = table(edited, PATH);
  assert.equal(found.string('command'), '/opt/local host/localhost');
  assert.deepEqual(found.strings('args'), ['mcp']);
  assert.equal(remove(edited, PATH), text);
});

test('starts an empty file', () => {
  assert.equal(set('', PATH, entries()), '[mcp_servers.localhost]\ncommand = "/opt/local host/localhost"\nargs = ["mcp"]\n');
  assert.equal(set('a = 1', PATH, entries()), 'a = 1\n\n[mcp_servers.localhost]\ncommand = "/opt/local host/localhost"\nargs = ["mcp"]\n');
});

test('repoints a table, keeping its other keys and comments', () => {
  const text =
    '[mcp_servers.localhost] # mine\ncommand = \'/Users/me/.cargo/bin/localhost\' # built by hand\nargs = [\n  "mcp",\n  # more\n  "--verbose",\n]\nstartup_timeout_sec = 20\n\n[mcp_servers.localhost.env]\nA = "1"\n\n[profiles.fast]\nmodel = "o4"\n';
  const edited = set(text, PATH, entries());
  assert.equal(
    edited,
    '[mcp_servers.localhost] # mine\ncommand = "/opt/local host/localhost" # built by hand\nargs = ["mcp"]\nstartup_timeout_sec = 20\n\n[mcp_servers.localhost.env]\nA = "1"\n\n[profiles.fast]\nmodel = "o4"\n',
  );
  assert.equal(remove(edited, PATH), '[profiles.fast]\nmodel = "o4"\n');
});

test('adds missing keys under the header', () => {
  assert.equal(set('[mcp_servers.localhost]\nenabled = true\n', PATH, entries()), '[mcp_servers.localhost]\ncommand = "/opt/local host/localhost"\nargs = ["mcp"]\nenabled = true\n');
});

test('ignores brackets in strings and arrays', () => {
  const text = 'notes = """\n[mcp_servers.localhost]\n"""\nmatrix = [\n  [1, 2],\n]\nname = "[x]"\n';
  assert.equal(table(text, PATH), undefined);
  assert.equal(remove(text, PATH), undefined);
  assert.ok(set(text, PATH, entries()).startsWith(text));
});

test('refuses a server written another way', () => {
  for (const text of [
    '[mcp_servers]\nlocalhost = { command = "x" }\n',
    'mcp_servers.localhost.command = "x"\n',
    'mcp_servers = { localhost = { command = "x" } }\n',
    '[mcp_servers]\n"localhost".command = "x"\n',
  ]) {
    assert.throws(() => set(text, PATH, entries()), /another way/, text);
    assert.throws(() => table(text, PATH), /another way/, text);
  }
  assert.doesNotThrow(() => set('[mcp_servers]\ngithub = { command = "gh" }\n', PATH, entries()));
});

test('reads quoted and escaped values', () => {
  const found = table('[mcp_servers."localhost"]\ncommand = "C:\\\\Program Files\\\\localhost.exe"\nargs = [\'mcp\']\n', PATH);
  assert.equal(found.string('command'), 'C:\\Program Files\\localhost.exe');
  assert.deepEqual(found.strings('args'), ['mcp']);
  assert.equal(renderString('C:\\a'), '"C:\\\\a"');
  assert.equal(renderStrings(['mcp', '--read-only']), '["mcp", "--read-only"]');
});

test('refuses what it can’t read', () => {
  assert.throws(() => set('a = """\nopen', PATH, entries()), /isn’t closed/);
  assert.throws(() => set('[unclosed\n', PATH, entries()), /table header/);
});
