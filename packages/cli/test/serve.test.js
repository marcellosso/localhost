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

/**
 * Runs `mcp serve` as on `platform`, with a fake `spawn`, signals and re-raise. The fixture's command is where this computer's app would
 * put it, a real path here, and is the one candidate, so only what `serve` does differs by `platform`.
 */
function serve({ argv = [], app = true, readOnly = true, platform = 'linux' } = {}) {
  const spawned = fakeSpawn();
  const signals = new EventEmitter();
  const raised = [];
  const io = { platform, candidates: () => [home.command], spawn: spawned.spawn, signals, raise: (signal) => raised.push(signal) };
  const home = fixture({ app, readOnly, io });
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
  const { running, calls, signals } = serve({ platform: 'win32' });
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

/** Runs `node -e script -- ...argv` in `cwd`, writing `input` to its stdin: `{ code, out, err }`. */
async function runNode(script, argv, { cwd, input }) {
  const child = spawn(process.execPath, ['--input-type=module', '-e', script, '--', ...argv], { cwd, stdio: ['pipe', 'pipe', 'pipe'] });
  let out = '';
  let err = '';
  child.stdout.on('data', (chunk) => (out += chunk));
  child.stderr.on('data', (chunk) => (err += chunk));
  child.stdin.end(input);
  const code = await new Promise((resolve) => child.on('close', resolve));
  return { code, out, err };
}

const cli = new URL('../src/cli.js', import.meta.url).href;
const request = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n';

// A real server on every OS: Node itself as the app's command, which `mcp` makes run the script named mcp in the working folder. It echoes
// stdin and exits with 3 once stdin ends.
test('end to end: runs the server with the client’s stdin and stdout, and exits as it does', async () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'getlocalhost-serve-'));
  fs.writeFileSync(
    path.join(folder, 'mcp'),
    "process.stderr.write(`args: ${['mcp', ...process.argv.slice(2)].join(' ')}\\n`);\nprocess.stdin.on('end', () => (process.exitCode = 3));\nprocess.stdin.pipe(process.stdout);\n",
  );
  const script = `import { main } from ${JSON.stringify(cli)};
const probe = (command, args) => ({ output: args[0] === '--version' ? 'localhost 9.9.9\\n' : '  mcp [--read-only]   Serve Projects\\n' });
process.exitCode = await main(['mcp', 'serve', ...process.argv.slice(1)], { candidates: () => [process.execPath], spotlight: () => [], probe });`;
  for (const argv of [[], ['--read-only']]) {
    const { code, out, err } = await runNode(script, argv, { cwd: folder, input: request });
    assert.equal(code, 3, err);
    assert.equal(out, request);
    assert.equal(err.trim(), `args: ${['mcp', ...argv].join(' ')}`);
  }
});

// The app's command as findApp checks it: a `localhost` script that answers --version and --help as the app's does, and echoes stdin as `mcp`.
test('end to end: finds and runs a localhost command', { skip: process.platform === 'win32' && 'the fake command is a shell script; the test above runs on Windows' }, async () => {
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
  const script = `import { main } from ${JSON.stringify(cli)};
process.exitCode = await main(['mcp', 'serve', ...process.argv.slice(1)], { candidates: () => [${JSON.stringify(command)}], spotlight: () => [] });`;

  for (const argv of [[], ['--read-only']]) {
    const { code, out, err } = await runNode(script, argv, { input: request });
    assert.equal(code, 0, err);
    assert.equal(out, request);
    assert.equal(err.trim(), `args: ${['mcp', ...argv].join(' ')}`);
  }
});
