// The MCP Registry's entry, server.json at the repository's root, names this
// package at the version npm has: publish both together.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { test } from 'node:test';

const read = (file) => JSON.parse(fs.readFileSync(new URL(file, import.meta.url), 'utf8'));
const pkg = read('../package.json');

test('server.json is this package, at its version, running mcp serve', () => {
  const server = read('../../../server.json');
  assert.equal(server.name, pkg.mcpName);
  assert.equal(server.version, pkg.version);
  assert.equal(server.packages.length, 1);
  const [entry] = server.packages;
  assert.equal(entry.identifier, pkg.name);
  assert.equal(entry.version, pkg.version);
  assert.deepEqual(entry.packageArguments.map((argument) => argument.value), ['mcp', 'serve']);
  assert.ok(server.description.length <= 100, 'the registry takes 100 characters at most');
});

test('the example .mcp.json runs mcp serve', () => {
  assert.deepEqual(read('../../../.mcp.json').mcpServers.localhost.args, ['-y', pkg.name, 'mcp', 'serve']);
});
