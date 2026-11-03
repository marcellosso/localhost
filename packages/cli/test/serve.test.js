// `mcp serve`: runs the app's `localhost mcp` for MCP clients started with
// npx, keeping stdout for the server alone.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { EventEmitter } from 'node:events';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { parse } from '../src/cli.js';
import { fixture } from './fixture.js';

/** A `spawn` that records its calls and hands back a server the test ends, and the signals sent to it. */
function fakeSpawn() {
  const calls = [];
  const spawnServer = (command, args, options) => {
    const child = new EventEmitter();
    child.killed = [];
    child.kill = (signal) => child.killed.push(signal);
    calls.push({ command, args, options, child });
    return child;
  };
  return { calls, spawn: spawnServer };
}

/** Runs `mcp serve` against a fixture, with a fake `spawn`, signals and re-raise. */
function serve({ argv = [], app = true, readOnly = true, platform = 'linux', io = {} } = {}) {
  const spawned = fakeSpawn();
  const signals = new EventEmitter();
  const raised = [];
  const home = fixture({ app, readOnly, platform, io: { spawn: spawned.spawn, signals, raise: (signal) => raised.push(signal), ...io } });
  const running = home.run('mcp', 'serve', ...argv);
  return { home, running, calls: spawned.calls, signals, raised };
}

test('parses mcp serve, and --read-only only for install and serve', () => {
  assert.deepEqual(parse(['mcp', 'serve']), { command: 'serve', clients: [], readOnly: false, dryRun: false });
  assert.deepEqual(parse(['mcp', 'serve', '--read-only']), { command: 'serve', clients: [], readOnly: true, dryRun: false });
  assert.equal(parse(['mcp', 'install', '--read-only']).readOnly, true);
  for (const command of ['uninstall', 'status']) assert.match(parse(['mcp', command, '--read-only']).error, /--read-only is for mcp install and mcp serve/);
  assert.match(parse(['mcp', 'serve', '--client', 'cursor']).error, /only --read-only/);
  assert.match(parse(['mcp', 'serve', '--dry-run']).error, /only --read-only/);
});

test('runs the app’s localhost mcp with the client’s stdin and stdout', async () => {
  const { home, running, calls } = serve();
  assert.equal(calls.length, 1);
  assert.equal(calls[0].command, home.command);
  assert.deepEqual(calls[0].args, ['mcp']);
  assert.deepEqual(calls[0].options, { stdio: 'inherit', windowsHide: true });
  calls[0].child.emit('exit', 0, null);
  assert.deepEqual(await running, { code: 0, out: '', err: '' });
});

test('passes --read-only on', async () => {
  const { running, calls } = serve({ argv: ['--read-only'] });
  assert.deepEqual(calls[0].args, ['mcp', '--read-only']);
  calls[0].child.emit('exit', 0, null);
  assert.equal((await running).code, 0);
});

test('without the app, says how to install it on stderr only, and fails', async () => {
  for (const platform of ['darwin', 'win32', 'linux']) {
    const { running, calls } = serve({ app: false, platform });
    const { code, out, err } = await running;
    assert.equal(code, 1);
    assert.equal(out, '', platform);
    assert.match(err, /The localhost app isn’t installed/);
    assert.match(err, /https:\/\/www\.getlocalhost\.app/);
    assert.equal(calls.length, 0);
  }
});

test('--read-only fails when the app’s command doesn’t have it', async () => {
  const { running, calls } = serve({ argv: ['--read-only'], readOnly: false });
  const { code, out, err } = await running;
  assert.equal(code, 1);
  assert.equal(out, '');
  assert.match(err, /localhost 1\.2\.3 has no `localhost mcp --read-only` yet/);
  assert.equal(calls.length, 0);
});

test('exits with the server’s exit code', async () => {
  const { running, calls } = serve();
  calls[0].child.emit('exit', 3, null);
  assert.equal((await running).code, 3);
});

test('forwards SIGINT, SIGTERM and SIGHUP to the server, and stops listening once it ends', async () => {
  const { running, calls, signals } = serve();
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) signals.emit(signal, signal);
  assert.deepEqual(calls[0].child.killed, ['SIGINT', 'SIGTERM', 'SIGHUP']);
  calls[0].child.emit('exit', 0, null);
  await running;
  assert.equal(signals.listenerCount('SIGTERM'), 0);
});

test('on Windows, leaves Ctrl-C to the console the server shares', async () => {
  // The Linux fixture's command, found as if this were Windows: a Windows path wouldn't be one on this computer.
  const linuxCommand = (context) => [path.join(context.env.LOCALHOST_STORAGE_ROOT, 'bin', 'localhost')];
  const { running, calls, signals } = serve({ io: { platform: 'win32', candidates: linuxCommand } });
  signals.emit('SIGINT', 'SIGINT');
  assert.deepEqual(calls[0].child.killed, []);
  calls[0].child.emit('exit', 0, null);
  assert.equal((await running).code, 0);
});

test('ends the way the server did when a signal killed it', async () => {
  const { running, calls, raised } = serve();
  calls[0].child.emit('exit', null, 'SIGTERM');
  assert.equal((await running).code, 128 + os.constants.signals.SIGTERM);
  assert.deepEqual(raised, ['SIGTERM']);
});

test('a server that can’t be started is an error on stderr', async () => {
  const { home, running, calls } = serve();
  calls[0].child.emit('error', new Error('spawn EACCES'));
  const { code, out, err } = await running;
  assert.equal(code, 1);
  assert.equal(out, '');
  assert.equal(err, `Couldn’t run ${home.command}: spawn EACCES`);
});

// A real server: a `localhost` script that answers --version and --help as the app's does, and echoes stdin as `mcp`.
test('end to end: stdout carries only the server’s output', { skip: process.platform === 'win32' && 'the fake command is a shell script' }, async () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'getlocalhost-serve-'));
  const command = path.join(folder, 'localhost');
  fs.writeFileSync(
    command,
    [
      '#!/bin/sh',
      'case "$1" in',
      '  --version) echo "localhost 9.9.9" ;;',
      '  --help) echo "  mcp [--read-only]   Serve Projects" ;;',
      '  mcp) echo "args: $*" >&2; exec /bin/cat ;;',
      '  *) exit 64 ;;',
      'esac',
      '',
    ].join('\n'),
    { mode: 0o755 },
  );
  const cli = new URL('../src/cli.js', import.meta.url).href;
  const script = `import { main } from ${JSON.stringify(cli)};
process.exitCode = await main(['mcp', 'serve', ...process.argv.slice(1)], { candidates: () => [${JSON.stringify(command)}], spotlight: () => [] });`;
  const request = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n';

  for (const argv of [[], ['--read-only']]) {
    const child = spawn(process.execPath, ['--input-type=module', '-e', script, '--', ...argv], { stdio: ['pipe', 'pipe', 'pipe'] });
    let out = '';
    let err = '';
    child.stdout.on('data', (chunk) => (out += chunk));
    child.stderr.on('data', (chunk) => (err += chunk));
    child.stdin.end(request);
    const code = await new Promise((resolve) => child.on('close', resolve));
    assert.equal(code, 0, err);
    assert.equal(out, request);
    assert.equal(err.trim(), `args: ${['mcp', ...argv].join(' ')}`);
  }
});
