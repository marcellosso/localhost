// `mcp install`, `uninstall` and `status` against a home folder of agents:
// what is written, what stays, and what is refused.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';

import { renderString } from '../src/toml-edit.js';
import { SKILL, fixture } from './fixture.js';

const claudeEntry = (command, args = ['mcp']) => `"localhost": {\n      "type": "stdio",\n      "command": ${JSON.stringify(command)},\n      "args": [\n${args.map((arg) => `        "${arg}"`).join(',\n')}\n      ]\n    }`;
const tomlEntry = (command, args = '["mcp"]') => `[mcp_servers.localhost]\ncommand = ${renderString(command)}\nargs = ${args}\n`;

test('creates missing configuration files with the localhost server and installs the skill once', () => {
  const home = fixture();
  const { code, out } = home.run('mcp', 'install');
  assert.equal(code, 0, out);
  assert.equal(home.read('.claude.json'), `{\n  "mcpServers": {\n    ${claudeEntry(home.command)}\n  }\n}\n`);
  assert.equal(home.read('.cursor', 'mcp.json'), `{\n  "mcpServers": {\n    ${claudeEntry(home.command)}\n  }\n}\n`);
  assert.equal(home.read('.codex', 'config.toml'), tomlEntry(home.command));
  assert.equal(home.read('.claude', 'skills', 'localhost-mcp', 'SKILL.md'), SKILL);
  assert.equal(home.read('.agents', 'skills', 'localhost-mcp', 'SKILL.md'), SKILL);
  // Cursor reads Claude Code's copy.
  assert.equal(home.read('.cursor', 'skills', 'localhost-mcp', 'SKILL.md'), null);
  assert.match(out, /Claude Code: Connected\. New Claude Code sessions can use localhost\./);
  assert.match(out, /Codex: Connected\. Restart Codex to use localhost\./);
  // Nothing was there to back up.
  assert.equal(fs.existsSync(path.join(home.env.LOCALHOST_STORAGE_ROOT, 'agent-backups')), false);
});

test('adds the server beside other servers and settings, every other character as it was', () => {
  const home = fixture();
  const claude = '{\n  "numStartups": 12,\n  "mcpServers": {\n    "github": {\n      "command": "gh",\n      "args": ["mcp"]\n    }\n  },\n  "projects": {\n    "/src/app": { "allowedTools": [] }\n  }\n}\n';
  const cursor = '// mine\n{\n  "mcpServers": {\n    "Sanity": { "type": "http", "url": "https://mcp.sanity.io" },\n  },\n}\n';
  const codex = 'model = "gpt-5"  # default\n\n[mcp_servers.github]\ncommand = "gh"\n\n[profiles.fast]\nmodel = "o4"\n';
  home.write(['.claude.json'], claude);
  home.write(['.cursor', 'mcp.json'], cursor);
  home.write(['.codex', 'config.toml'], codex);

  assert.equal(home.run('mcp', 'install').code, 0);
  assert.equal(
    home.read('.claude.json'),
    `{\n  "numStartups": 12,\n  "mcpServers": {\n    "github": {\n      "command": "gh",\n      "args": ["mcp"]\n    },\n    ${claudeEntry(home.command)}\n  },\n  "projects": {\n    "/src/app": { "allowedTools": [] }\n  }\n}\n`,
  );
  const editedCursor = home.read('.cursor', 'mcp.json');
  assert.ok(editedCursor.startsWith('// mine\n{\n  "mcpServers": {\n    "Sanity": { "type": "http", "url": "https://mcp.sanity.io" },\n    "localhost": {'), editedCursor);
  assert.ok(editedCursor.endsWith('    },\n  },\n}\n'), editedCursor);
  assert.equal(home.read('.codex', 'config.toml'), `${codex}\n${tomlEntry(home.command)}`);

  // Each file as it was before the first change, where the app keeps its backups.
  const backups = path.join(home.env.LOCALHOST_STORAGE_ROOT, 'agent-backups');
  assert.equal(fs.readFileSync(path.join(backups, 'claude-code', '.claude.json'), 'utf8'), claude);
  assert.equal(fs.readFileSync(path.join(backups, 'cursor', 'mcp.json'), 'utf8'), cursor);
  assert.equal(fs.readFileSync(path.join(backups, 'codex', 'config.toml'), 'utf8'), codex);

  // Uninstall takes away only what install added.
  assert.equal(home.run('mcp', 'uninstall').code, 0);
  assert.equal(home.read('.claude.json'), claude);
  assert.equal(home.read('.cursor', 'mcp.json'), cursor);
  assert.equal(home.read('.codex', 'config.toml'), codex);
  assert.equal(home.read('.claude', 'skills', 'localhost-mcp', 'SKILL.md'), null);
  assert.equal(home.read('.agents', 'skills', 'localhost-mcp', 'SKILL.md'), null);
  assert.equal(fs.existsSync(home.file('.claude', 'skills', 'localhost-mcp')), false);
});

test('running it again changes nothing, and keeps the first backup', () => {
  const home = fixture();
  home.write(['.cursor', 'mcp.json'], '{"mcpServers": {}}');
  home.run('mcp', 'install');
  const files = ['.claude.json', path.join('.cursor', 'mcp.json'), path.join('.codex', 'config.toml')].map((file) => [file, home.read(file)]);
  const stamps = files.map(([file]) => fs.statSync(home.file(file)).mtimeMs);

  const again = home.run('mcp', 'install');
  assert.equal(again.code, 0);
  assert.match(again.out, /Claude Code: already connected; nothing changed\./);
  assert.match(again.out, /Cursor: already connected; nothing changed\./);
  assert.match(again.out, /Codex: already connected; nothing changed\./);
  for (const [index, [file, text]] of files.entries()) {
    assert.equal(home.read(file), text);
    assert.equal(fs.statSync(home.file(file)).mtimeMs, stamps[index], file);
  }
  assert.equal(fs.readFileSync(path.join(home.env.LOCALHOST_STORAGE_ROOT, 'agent-backups', 'cursor', 'mcp.json'), 'utf8'), '{"mcpServers": {}}');
});

test('repoints a server that runs another localhost command, keeping the rest of its entry', () => {
  const home = fixture({ agents: ['.cursor', '.codex'] });
  home.write(['.cursor', 'mcp.json'], '{\n  "mcpServers": {\n    "localhost": {\n      "command": "/home/me/.cargo/bin/localhost",\n      "args": ["mcp"],\n      "env": { "A": "1" }\n    }\n  }\n}\n');
  home.write(['.codex', 'config.toml'], '[mcp_servers.localhost]\ncommand = "/home/me/.cargo/bin/localhost" # mine\nargs = ["mcp"]\nstartup_timeout_sec = 20\n');
  assert.match(home.run('mcp', 'status').out, /It runs (another localhost command, )?\/home\/me\/\.cargo\/bin\/localhost/);

  assert.equal(home.run('mcp', 'install').code, 0);
  assert.equal(
    home.read('.cursor', 'mcp.json'),
    `{\n  "mcpServers": {\n    "localhost": {\n      "command": ${JSON.stringify(home.command)},\n      "args": [\n        "mcp"\n      ],\n      "env": { "A": "1" }\n    }\n  }\n}\n`,
  );
  assert.equal(home.read('.codex', 'config.toml'), `[mcp_servers.localhost]\ncommand = ${renderString(home.command)} # mine\nargs = ["mcp"]\nstartup_timeout_sec = 20\n`);
  // With Claude Code not installed, Cursor keeps its own copy of the skill; Codex's folder has one too, so Cursor's goes.
  assert.equal(home.read('.cursor', 'skills', 'localhost-mcp', 'SKILL.md'), null);
  assert.equal(home.read('.agents', 'skills', 'localhost-mcp', 'SKILL.md'), SKILL);
});

test('replaces a localhost server that isn’t a command, such as a URL', () => {
  const home = fixture({ agents: ['.cursor'] });
  home.write(['.cursor', 'mcp.json'], '{"mcpServers": {"localhost": {"url": "http://localhost:3000/mcp"}}}');
  assert.equal(home.run('mcp', 'install').code, 0);
  const entry = JSON.parse(home.read('.cursor', 'mcp.json')).mcpServers.localhost;
  assert.deepEqual(entry, { type: 'stdio', command: home.command, args: ['mcp'] });
  // Cursor alone keeps its own copy of the skill.
  assert.equal(home.read('.cursor', 'skills', 'localhost-mcp', 'SKILL.md'), SKILL);
});

test('--read-only runs localhost mcp --read-only, and installing again keeps it read-only', () => {
  const home = fixture();
  assert.equal(home.run('mcp', 'install', '--read-only').code, 0);
  const read = () => [JSON.parse(home.read('.claude.json')).mcpServers.localhost.args, JSON.parse(home.read('.cursor', 'mcp.json')).mcpServers.localhost.args];
  assert.deepEqual(read(), [['mcp', '--read-only'], ['mcp', '--read-only']]);
  assert.equal(home.read('.codex', 'config.toml'), tomlEntry(home.command, '["mcp", "--read-only"]'));
  assert.match(home.run('mcp', 'status').out, /It is read-only: it can’t add, edit or remove Projects and Services\./);

  const again = home.run('mcp', 'install');
  assert.match(again.out, /already connected, read-only; nothing changed/);
  assert.deepEqual(read(), [['mcp', '--read-only'], ['mcp', '--read-only']]);
});

test('--read-only is refused, writing nothing, when the app’s command doesn’t have it', () => {
  const home = fixture({ readOnly: false });
  const { code, err } = home.run('mcp', 'install', '--read-only');
  assert.equal(code, 1);
  assert.match(err, /localhost 1\.2\.3 has no `localhost mcp --read-only` yet/);
  assert.equal(home.read('.claude.json'), null);
});

test('without the app it says how to install it for this OS and writes nothing', () => {
  for (const platform of ['darwin', 'win32', 'linux']) {
    const home = fixture({ app: false, platform });
    const { code, err } = home.run('mcp', 'install');
    assert.equal(code, 1, platform);
    assert.match(err, /The localhost app isn’t installed/);
    assert.match(err, /https:\/\/www\.getlocalhost\.app/);
    assert.match(err, /https:\/\/github\.com\/marcellosso\/localhost\/releases\/latest/);
    assert.equal(err.includes('brew install --cask marcellosso/tap/localhost'), platform === 'darwin', platform);
    if (platform === 'win32') assert.match(err, /x64-setup\.exe/);
    if (platform === 'linux') assert.match(err, /\.deb[\s\S]*\.rpm[\s\S]*AppImage/);
    assert.deepEqual(fs.readdirSync(home.home).sort(), ['.claude', '.codex', '.cursor'], platform);
    assert.deepEqual(fs.readdirSync(home.file('.claude')), []);
  }
});

test('leaves a file it can’t read alone, says why, and connects the other agents', () => {
  const home = fixture();
  home.write(['.cursor', 'mcp.json'], '{"mcpServers": {"a": 1,,}}');
  home.write(['.codex', 'config.toml'], '[mcp_servers]\nlocalhost = { command = "x" }\n');
  const { code, out, err } = home.run('mcp', 'install');
  assert.equal(code, 1);
  assert.match(err, /Cursor: localhost can’t edit .*mcp\.json: a key was expected on line 1\. Fix the file, or set up the server by hand\./);
  assert.match(err, /Codex: localhost can’t edit .*config\.toml: it sets “mcp_servers\.localhost” another way than as a \[mcp_servers\.localhost\] table/);
  assert.equal(home.read('.cursor', 'mcp.json'), '{"mcpServers": {"a": 1,,}}');
  assert.equal(home.read('.codex', 'config.toml'), '[mcp_servers]\nlocalhost = { command = "x" }\n');
  assert.match(out, /Claude Code: Connected/);
  assert.match(home.run('mcp', 'status').out, /Cursor\s+localhost can’t edit/);
});

test('--dry-run prints the change and writes nothing', () => {
  const home = fixture({ agents: ['.cursor'] });
  home.write(['.cursor', 'mcp.json'], '{\n  "mcpServers": {\n    "github": { "command": "gh" }\n  }\n}\n');
  const { code, out } = home.run('mcp', 'install', '--dry-run');
  assert.equal(code, 0);
  assert.match(out, /dry run: nothing is changed/);
  assert.match(out, /Cursor: would connect it in .*mcp\.json\./);
  assert.match(out, /-    "github": \{ "command": "gh" \}\n\s+\+    "github": \{ "command": "gh" \},\n\s+\+    "localhost": \{/);
  assert.match(out, /Would add .*SKILL\.md/);
  assert.equal(home.read('.cursor', 'mcp.json'), '{\n  "mcpServers": {\n    "github": { "command": "gh" }\n  }\n}\n');
  assert.equal(home.read('.cursor', 'skills', 'localhost-mcp', 'SKILL.md'), null);
  // No backup either. (On Linux the data folder exists already: the fixture's app command is the AppImage's copy in it.)
  assert.equal(fs.existsSync(path.join(home.env.LOCALHOST_STORAGE_ROOT, 'agent-backups')), false);
});

test('changes Claude Code’s servers through claude mcp when it is installed', () => {
  const home = fixture({ agents: ['.claude'], claude: '/bin/claude' });
  assert.equal(home.run('mcp', 'install').code, 0);
  const entry = JSON.stringify({ type: 'stdio', command: home.command, args: ['mcp'] });
  assert.deepEqual(home.claudeCalls, [['/bin/claude', 'mcp', 'add-json', '--scope', 'user', 'localhost', entry]]);
  assert.equal(home.read('.claude.json'), null);

  // An entry of the user's is removed, then added back pointed at the app, with its env.
  home.claudeCalls.length = 0;
  home.write(['.claude.json'], '{"mcpServers": {"localhost": {"type": "stdio", "command": "localhost", "args": ["mcp"], "env": {"A": "1"}}}}');
  assert.equal(home.run('mcp', 'install', '--read-only').code, 0);
  assert.deepEqual(home.claudeCalls, [
    ['/bin/claude', 'mcp', 'remove', '--scope', 'user', 'localhost'],
    ['/bin/claude', 'mcp', 'add-json', '--scope', 'user', 'localhost', JSON.stringify({ type: 'stdio', command: home.command, args: ['mcp', '--read-only'], env: { A: '1' } })],
  ]);

  home.claudeCalls.length = 0;
  home.run('mcp', 'uninstall');
  assert.deepEqual(home.claudeCalls, [['/bin/claude', 'mcp', 'remove', '--scope', 'user', 'localhost']]);
});

test('--client connects only the agents named', () => {
  const home = fixture();
  const { code, out } = home.run('mcp', 'install', '--client', 'cursor');
  assert.equal(code, 0);
  assert.doesNotMatch(out, /Claude Code|Codex/);
  assert.equal(home.read('.claude.json'), null);
  assert.equal(JSON.parse(home.read('.cursor', 'mcp.json')).mcpServers.localhost.command, home.command);
  // Alone, Cursor has its own copy of the skill; connecting Claude Code moves it to Claude Code's folder.
  assert.equal(home.read('.cursor', 'skills', 'localhost-mcp', 'SKILL.md'), SKILL);
  home.run('mcp', 'install', '--client=claude-code');
  assert.equal(home.read('.cursor', 'skills', 'localhost-mcp', 'SKILL.md'), null);
  assert.equal(home.read('.claude', 'skills', 'localhost-mcp', 'SKILL.md'), SKILL);
  // Disconnecting Claude Code gives the connected Cursor its own again.
  home.run('mcp', 'uninstall', '--client', 'claude-code');
  assert.equal(home.read('.cursor', 'skills', 'localhost-mcp', 'SKILL.md'), SKILL);
  assert.equal(home.read('.claude', 'skills', 'localhost-mcp', 'SKILL.md'), null);
});

test('an agent named but not installed, or not known, is an error', () => {
  const home = fixture({ agents: ['.cursor'] });
  const missing = home.run('mcp', 'install', '--client', 'codex');
  assert.equal(missing.code, 1);
  assert.match(missing.err, /Codex isn’t on this computer: there is no .*\.codex\./);
  assert.equal(home.read('.codex', 'config.toml'), null);

  const unknown = home.run('mcp', 'install', '--client', 'vscode');
  assert.equal(unknown.code, 2);
  assert.match(unknown.err, /Unknown agent “vscode”/);
  assert.equal(home.run('mcp').code, 2);
  assert.equal(home.run('mcp', 'uninstall', '--read-only').code, 2);
});

test('no agents on this computer', () => {
  const home = fixture({ agents: [] });
  const { code, err } = home.run('mcp', 'install');
  assert.equal(code, 1);
  assert.match(err, /No coding agents found on this computer/);
});

test('Claude Desktop gets the server without a type or a skill', { skip: process.platform === 'linux' && 'there is no Claude Desktop for Linux' }, () => {
  const folder = process.platform === 'win32' ? path.join('AppData', 'Roaming', 'Claude') : path.join('Library', 'Application Support', 'Claude');
  const home = fixture({ agents: [folder] });
  home.write([folder, 'claude_desktop_config.json'], '{\n  "preferences": { "sidebarMode": "chat" }\n}\n');
  const { code, out } = home.run('mcp', 'install', '--client', 'claude');
  assert.equal(code, 0, out);
  assert.doesNotMatch(out, /Claude Code/);
  assert.equal(
    home.read(folder, 'claude_desktop_config.json'),
    `{\n  "preferences": { "sidebarMode": "chat" },\n  "mcpServers": {\n    "localhost": {\n      "command": ${JSON.stringify(home.command)},\n      "args": [\n        "mcp"\n      ]\n    }\n  }\n}\n`,
  );
  assert.match(out, /Quit and reopen Claude Desktop/);
  assert.match(home.run('mcp', 'status', '--client', 'claude-desktop').out, /Claude Desktop\s+Connected\./);
});

test('status works without the app, and says how to install it', () => {
  const home = fixture({ app: false, platform: 'darwin' });
  home.write(['.cursor', 'mcp.json'], `{"mcpServers": {"localhost": {"command": ${JSON.stringify(path.join(home.home, 'gone', 'localhost'))}, "args": ["mcp"]}}}`);
  const { code, out } = home.run('mcp', 'status');
  assert.equal(code, 0);
  assert.match(out, /^The localhost app isn’t installed\./);
  assert.match(out, /Claude Code\s+Not connected\./);
  assert.match(out, /Cursor\s+It runs .*localhost, which no longer exists\./);
  assert.match(out, /brew install --cask marcellosso\/tap\/localhost/);
});
