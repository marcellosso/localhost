// A home folder of agents to connect, and the localhost app's command in
// it, where each OS's app puts it relative to the home folder.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { candidates } from '../src/app.js';
import { main } from '../src/cli.js';

export const SKILL = '---\nname: localhost-mcp\ndescription: test\n---\n\nUse localhost.\n';

/**
 * `agents`: the agents' folders to create. `app`: whether the app is installed; `readOnly`: whether its command has `mcp --read-only`.
 * `claude`: the `claude` command Claude Code is changed with (`null`: edit the file).
 */
export function fixture({ agents = ['.claude', '.cursor', '.codex'], app = true, readOnly = true, claude = null, platform = process.platform } = {}) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'getlocalhost-'));
  const env = {
    PATH: '',
    LOCALHOST_STORAGE_ROOT: path.join(home, 'data'),
    APPDATA: path.join(home, 'AppData', 'Roaming'),
    LOCALAPPDATA: path.join(home, 'AppData', 'Local'),
  };
  const command =
    platform === 'darwin'
      ? path.join(home, 'Applications', 'localhost.app', 'Contents', 'Helpers', 'localhost')
      : platform === 'win32'
        ? path.join(env.LOCALAPPDATA, 'localhost', 'localhost.exe')
        : path.join(env.LOCALHOST_STORAGE_ROOT, 'bin', 'localhost');
  if (app) write(command, 'the localhost command');
  for (const folder of agents) fs.mkdirSync(path.join(home, folder), { recursive: true });
  const claudeCalls = [];
  const file = (...parts) => path.join(home, ...parts);
  return {
    home,
    env,
    command,
    claudeCalls,
    file,
    read: (...parts) => (fs.existsSync(file(...parts)) ? fs.readFileSync(file(...parts), 'utf8') : null),
    write: (parts, text) => write(file(...parts), text),
    /** Runs the command line; `{ code, out, err }`, each a string. */
    run(...argv) {
      const out = [];
      const err = [];
      const code = main(argv, {
        platform,
        home,
        env,
        skill: SKILL,
        spotlight: () => [],
        // Only where the app would be under this home folder: not an app installed on this machine.
        candidates: (context) => [...candidates(context)].filter((candidate) => candidate.startsWith(home)),
        probe: (candidate, args) => {
          if (candidate !== command) return { error: 'not the fixture’s' };
          if (args[0] === '--version') return { output: 'localhost 1.2.3\n' };
          return { output: readOnly ? '  mcp [--read-only]   Serve Projects\n' : '  mcp   Serve Projects\n' };
        },
        claude,
        runClaude: (claudeCommand, args) => claudeCalls.push([claudeCommand, ...args]),
        out: (line) => out.push(line),
        err: (line) => err.push(line),
      });
      return { code, out: out.join('\n'), err: err.join('\n') };
    },
  };
}

function write(file, text) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text);
}
