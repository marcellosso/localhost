// `npx @getlocalhost/cli mcp install|uninstall|status`: connects coding agents
// to `localhost mcp` from a terminal, as Settings → Agents does in the app.

import fs from 'node:fs';
import os from 'node:os';

import { Agents, CLIENTS } from './agents.js';
import { brokenInstructions, findApp, installInstructions, storageRoot } from './app.js';
import { diff } from './diff.js';
import { Files, display } from './files.js';

const USAGE = `Usage: npx @getlocalhost/cli mcp <command> [options]

Connects your coding agents to localhost mcp, the MCP server of the localhost
app, as Settings → Agents does in the app. The app must be installed: agents
run the localhost command it comes with.

Commands:
  mcp install       Add the localhost MCP server, and the localhost-mcp skill,
                    to your agents. Every other server and setting stays.
  mcp uninstall     Remove them, and nothing else
  mcp status        Show where each agent stands

Options:
  --client <agent>  claude-code, claude-desktop, cursor or codex; claude for
                    both Claude apps. Repeat for more than one. Without it:
                    every agent found on this computer
  --read-only       Leave out the tools that add, edit or remove Projects and
                    Services (localhost mcp --read-only)
  --dry-run         Print what would change, and change nothing
  -h, --help        Show this
  -V, --version     Show this package's version

More: https://www.getlocalhost.app/docs/mcp`;

const ALIASES = { claude: ['claude-code', 'claude-desktop'] };

/**
 * Parses `argv` (without `node` and the script): `{ command, clients, readOnly, dryRun }`, `{ help }`, `{ version }`, or `{ error }`.
 * Each of `clients` is `{ slug, alias }`, `alias` when it came from `claude`, which means the Claude apps that are installed.
 */
export function parse(argv) {
  const options = { clients: [], readOnly: false, dryRun: false };
  const words = [];
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '-h' || argument === '--help' || argument === 'help') return { help: true };
    if (argument === '-V' || argument === '--version') return { version: true };
    if (argument === '--read-only') options.readOnly = true;
    else if (argument === '--dry-run') options.dryRun = true;
    else if (argument === '--client' || argument.startsWith('--client=')) {
      const value = argument === '--client' ? argv[++index] : argument.slice('--client='.length);
      if (!value) return { error: '--client needs an agent: claude-code, claude-desktop, cursor or codex.' };
      for (const name of value.split(',')) {
        const slugs = ALIASES[name] ?? (CLIENTS.includes(name) ? [name] : undefined);
        if (!slugs) return { error: `Unknown agent “${name}”. Use claude-code, claude-desktop, cursor or codex.` };
        for (const slug of slugs) {
          const existing = options.clients.find((request) => request.slug === slug);
          if (!existing) options.clients.push({ slug, alias: slugs.length > 1 });
          else if (slugs.length === 1) existing.alias = false;
        }
      }
    } else if (argument.startsWith('-')) return { error: `Unknown option ${argument}.` };
    else words.push(argument);
  }
  if (words[0] !== 'mcp' || words.length !== 2 || !['install', 'uninstall', 'status'].includes(words[1])) {
    return { error: words.length === 0 ? USAGE : `Unknown command “${words.join(' ')}”.\n\n${USAGE}` };
  }
  if (options.readOnly && words[1] !== 'install') return { error: '--read-only is for mcp install.' };
  return { command: words[1], ...options };
}

/**
 * Runs the command line, writing to `io.out` and `io.err`; returns the exit code. `io` also carries what tests replace: `platform`, `home`,
 * `env`, and the `candidates`, `probe`, `spotlight`, `claude` and `runClaude` of `findApp` and `Agents`.
 */
export function main(argv, io = {}) {
  const out = io.out ?? ((line) => process.stdout.write(`${line}\n`));
  const err = io.err ?? ((line) => process.stderr.write(`${line}\n`));
  const parsed = parse(argv);
  if (parsed.help || parsed.version) {
    out(parsed.help ? USAGE : packageVersion());
    return 0;
  }
  if (parsed.error) {
    err(parsed.error);
    return 2;
  }

  const platform = io.platform ?? process.platform;
  const home = io.home ?? homeFolder(platform);
  const env = io.env ?? process.env;
  const context = { platform, home, env, probe: io.probe, spotlight: io.spotlight, candidates: io.candidates };
  const app = findApp(context);
  const files = new Files({ dryRun: parsed.dryRun });
  const agents = new Agents({
    ...context,
    files,
    command: app.command,
    storageRoot: storageRoot(context),
    skill: io.skill ?? readSkill(),
    claude: io.claude,
    runClaude: io.runClaude,
  });

  if (parsed.command === 'status') return status(agents, app, parsed, { out, platform, env });
  // Nothing is written for agents to run a command that isn't there.
  const refusal = parsed.command === 'install' && refuse(app, parsed, platform, env);
  if (refusal) {
    refusal.forEach((line) => err(line));
    return 1;
  }

  let failed = false;
  const selected = [];
  if (parsed.clients.length === 0) {
    selected.push(...agents.all().filter((agent) => agents.isInstalled(agent)));
  }
  for (const { slug, alias } of parsed.clients) {
    const agent = agents.get(slug);
    if (!agent && !alias) {
      err('Claude Desktop isn’t available for Linux.');
      failed = true;
    } else if (agent && (!alias || agents.isInstalled(agent))) {
      selected.push(agent);
    }
  }
  if (selected.length === 0) {
    if (parsed.command === 'install') {
      err('No coding agents found on this computer: install Claude Code, Claude Desktop, Cursor or Codex, or name one with --client.');
      return 1;
    }
    out('No coding agents found on this computer.');
    return failed ? 1 : 0;
  }

  if (parsed.command === 'install') out(`localhost ${app.version}: ${display(app.command, home)}${parsed.dryRun ? ' (dry run: nothing is changed)' : ''}`);
  else if (parsed.dryRun) out('Dry run: nothing is changed.');
  out('');
  for (const agent of selected) {
    const before = agents.status(agent);
    const logged = files.log.length;
    let ran;
    try {
      if (parsed.command === 'install') ran = agents.connect(agent, { readOnly: parsed.readOnly });
      else ran = agents.disconnect(agent);
    } catch (error) {
      if (!error.reason) throw error;
      err(`${agent.name}: ${error.message}`);
      failed = true;
      continue;
    }
    const changes = files.log.slice(logged);
    const server = ran.length > 0 || changes.some((change) => change.file === agent.config);
    out(`${agent.name}: ${summary(parsed, agents, agent, before, { server, changed: server || changes.length > 0 })}`);
    for (const command of ran) out(`  ${parsed.dryRun ? 'Would run' : 'Ran'}: ${shellLine(command)}`);
    for (const change of changes) {
      const shown = display(change.file, home);
      if (change.file === agent.config) {
        out(`  ${parsed.dryRun ? 'Would change' : 'Changed'} ${shown}${parsed.dryRun ? ':' : ''}`);
        if (parsed.dryRun) for (const line of diff(shown, change.before, change.after)) out(`    ${line}`);
      } else {
        const [verb, done] = change.after === null ? ['remove', 'Removed'] : change.before === null ? ['add', 'Added'] : ['update', 'Updated'];
        out(`  ${parsed.dryRun ? `Would ${verb}` : done} ${shown}`);
      }
    }
  }
  return failed ? 1 : 0;
}

/** What happened to one agent, in a sentence. */
function summary(parsed, agents, agent, before, { server, changed }) {
  const where = display(agent.config, agents.home);
  if (parsed.command === 'uninstall') {
    if (!changed) return before.connection === 'notInstalled' ? 'not found on this computer.' : 'wasn’t connected; nothing changed.';
    if (!server) return parsed.dryRun ? 'would remove the localhost-mcp skill.' : 'removed the localhost-mcp skill.';
    return parsed.dryRun ? `would remove the localhost server from ${where}.` : `removed the localhost server from ${where}. Every other server stays.`;
  }
  if (!changed) return before.readOnly ? 'already connected, read-only; nothing changed.' : 'already connected; nothing changed.';
  if (parsed.dryRun) return `would connect it in ${where}.`;
  const after = agents.status(agent);
  if (after.connection !== 'connected') return after.summary;
  const backup = agents.backupPath(agent);
  const backedUp = fs.existsSync(backup) ? ` The file as it was before localhost first changed it is in ${display(backup, agents.home)}.` : '';
  return `${after.summary}${backedUp}`;
}

/** Why install can't go ahead, as lines to print, if it can't. */
function refuse(app, parsed, platform, env) {
  if (app.missing) return noApp(platform, env);
  if (app.broken) return brokenInstructions(platform, app.broken, app.error);
  if (parsed.readOnly && !app.readOnly) return [`localhost ${app.version} has no \`localhost mcp --read-only\` yet. Update the localhost app, then run this again.`];
  return undefined;
}

function status(agents, app, parsed, { out, platform, env }) {
  if (app.command) out(`localhost ${app.version}: ${display(app.command, agents.home)}`);
  else if (app.broken) brokenInstructions(platform, app.broken, app.error).forEach((line) => out(line));
  else out('The localhost app isn’t installed.');
  out('');
  const shown = agents.all().filter((agent) => parsed.clients.length === 0 || parsed.clients.some((request) => request.slug === agent.slug));
  const width = Math.max(...shown.map((agent) => agent.name.length)) + 2;
  for (const agent of shown) {
    const current = agents.status(agent);
    out(`${agent.name.padEnd(width)}${current.summary}`);
    if (current.connection === 'connected' || current.connection === 'needsUpdate') out(`${' '.repeat(width)}${display(agent.config, agents.home)}`);
  }
  if (app.missing) {
    out('');
    noApp(platform, env).forEach((line) => out(line));
  }
  return 0;
}

function noApp(platform, env) {
  const lines = installInstructions(platform);
  if (platform === 'linux' && env.WSL_DISTRO_NAME) {
    lines.push('', 'In WSL? The app runs on Windows: run this in PowerShell for agents on Windows. Agents inside a WSL distro are set up by hand: https://www.getlocalhost.app/docs/mcp#set-it-up-by-hand');
  }
  return lines;
}

/** A command as it would be typed in a shell. */
function shellLine(words) {
  return words.map((word) => (/^[\w./:=@-]+$/.test(word) ? word : `'${word.replaceAll("'", "'\\''")}'`)).join(' ');
}

/** The user's home folder; on Windows their profile folder, not Git Bash's `HOME`. */
function homeFolder(platform) {
  if (platform !== 'win32' && process.env.HOME) return process.env.HOME;
  return os.homedir();
}

/** The localhost-mcp skill: the package's copy (`npm pack` adds it), else the repository's. */
export function readSkill() {
  for (const file of ['../skill/SKILL.md', '../../../skills/localhost-mcp/SKILL.md']) {
    try {
      return fs.readFileSync(new URL(file, import.meta.url), 'utf8');
    } catch {}
  }
  throw new Error('This package has no localhost-mcp skill: build it with npm pack.');
}

function packageVersion() {
  return JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version;
}
