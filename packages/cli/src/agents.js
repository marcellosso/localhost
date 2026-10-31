// Connecting coding agents to `localhost mcp`: the `localhost` MCP server in
// each agent's configuration file and the localhost-mcp skill in its skills
// folder, as the app's Connect your agents does (core/localhost-core/src/agents),
// so the app sees them connected. Only localhost's own entries are ever
// changed, and each file is backed up to the same place the app backs it up
// to, before it is first edited. Claude Desktop, which the app doesn't
// connect, gets the server without the skill (it has no skills folder).

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

import { display } from './files.js';
import * as json from './json-edit.js';
import * as toml from './toml-edit.js';

const SERVER = 'localhost';
const SKILL_FOLDER = 'localhost-mcp';
const JSON_PATH = ['mcpServers', SERVER];
const TOML_PATH = ['mcp_servers', SERVER];
/** What the `localhost` server runs: `localhost mcp`, or with the tools that change the Project Configuration left out. */
export const ARGUMENTS = ['mcp'];
export const READ_ONLY_ARGUMENTS = ['mcp', '--read-only'];

/** The agents, in the order they are connected: Cursor after Claude Code, so it can read Claude Code's copy of the skill. */
export const CLIENTS = ['claude-code', 'claude-desktop', 'cursor', 'codex'];

/** Each agent's name, the folder whose presence says it is installed, its configuration file and its skills folder. */
function client(slug, { home, platform, env }) {
  const join = (...parts) => path.join(home, ...parts);
  switch (slug) {
    case 'claude-code':
      return { slug, name: 'Claude Code', folder: join('.claude'), config: join('.claude.json'), format: 'json', skills: join('.claude', 'skills'), next: 'New Claude Code sessions can use localhost.' };
    case 'cursor':
      return { slug, name: 'Cursor', folder: join('.cursor'), config: join('.cursor', 'mcp.json'), format: 'json', skills: join('.cursor', 'skills'), next: 'Cursor picks it up; check Cursor Settings › MCP.' };
    case 'codex':
      return { slug, name: 'Codex', folder: join('.codex'), config: join('.codex', 'config.toml'), format: 'toml', skills: join('.agents', 'skills'), next: 'Restart Codex to use localhost.' };
    case 'claude-desktop': {
      // https://modelcontextprotocol.io/quickstart/user; there is no Claude Desktop for Linux.
      let folder;
      if (platform === 'darwin') folder = join('Library', 'Application Support', 'Claude');
      else if (platform === 'win32') folder = path.join(env.APPDATA && path.isAbsolute(env.APPDATA) ? env.APPDATA : join('AppData', 'Roaming'), 'Claude');
      if (!folder) return undefined;
      return { slug, name: 'Claude Desktop', folder, config: path.join(folder, 'claude_desktop_config.json'), format: 'json', skills: undefined, untyped: true, next: 'Quit and reopen Claude Desktop to use localhost.' };
    }
    default:
      return undefined;
  }
}

/**
 * Sets up agents under `context.home`, pointed at `context.command` (the app's command; `undefined` when there is no app), with
 * `context.files` (a `Files`) for every read and write. `context.claude` is the `claude` command Claude Code's servers are changed with,
 * `null` to edit `~/.claude.json`, `undefined` to look for one; `context.runClaude(claude, args)` runs it.
 */
export class Agents {
  constructor(context) {
    this.context = context;
    this.files = context.files;
    this.home = context.home;
    this.command = context.command;
    this.skill = context.skill;
  }

  /** The agents this computer can have, as `client(…)` records. */
  all() {
    return CLIENTS.map((slug) => client(slug, this.context)).filter(Boolean);
  }

  get(slug) {
    return client(slug, this.context);
  }

  isInstalled(agent) {
    return this.files.exists(agent.folder) || (agent.slug === 'claude-code' && this.files.exists(agent.config));
  }

  /** Where the agent stands: `{ connection, summary, … }`, `connection` one of the app's (`connected`, `needsUpdate`, …). */
  status(agent) {
    const shown = (file) => display(file, this.home);
    const status = { agent, connection: 'notConnected', summary: '', configPath: agent.config, readOnly: false };
    if (!this.isInstalled(agent)) {
      return { ...status, connection: 'notInstalled', summary: `Not found on this computer: there is no ${shown(agent.folder)}.` };
    }
    let entry;
    try {
      entry = this.readEntry(agent);
    } catch (error) {
      if (!error.reason) throw error;
      return { ...status, connection: 'unreadable', summary: `localhost can’t edit ${shown(agent.config)}: ${error.message}. Fix the file, or set up the server by hand.` };
    }
    if (!entry) return { ...status, summary: 'Not connected.' };
    status.configuredCommand = entry.command;
    status.readOnly = sameArguments(entry.args, READ_ONLY_ARGUMENTS);
    const skill = agent.skills && this.skillState(agent);
    let problem;
    if (entry.command === undefined) {
      problem = 'Its localhost server has no command. Install again to fix it.';
    } else if (this.command ? entry.command !== this.command : !path.isAbsolute(entry.command) || !this.files.exists(entry.command)) {
      problem =
        path.isAbsolute(entry.command) && !this.files.exists(entry.command)
          ? `It runs ${shown(entry.command)}, which no longer exists. Install again to use the app’s command.`
          : `It runs another localhost command, ${shown(entry.command)}. Install again to use the app’s.`;
    } else if (!isOurs(entry.args)) {
      problem = 'Its localhost server doesn’t run `localhost mcp`. Install again to fix it.';
    } else if (skill === 'missing') {
      problem = 'The localhost-mcp skill isn’t installed. Install again to add it.';
    } else if (skill === 'changed') {
      problem = 'The localhost-mcp skill has changed. Install again to install this version.';
    }
    if (problem) return { ...status, connection: 'needsUpdate', summary: problem };
    let summary = `Connected. ${agent.next}`;
    if (status.readOnly) summary += ' It is read-only: it can’t add, edit or remove Projects and Services.';
    return { ...status, connection: 'connected', summary };
  }

  /** `current`, `changed` or `missing`: the skill where the agent reads it, which for Cursor may be Claude Code's or Codex's copy. */
  skillState(agent) {
    if (agent.slug === 'cursor' && this.sharedSkillForCursor()) return 'current';
    const text = this.files.read(skillPath(agent));
    if (text === null || !isOurSkill(text)) return 'missing';
    return text === this.skill ? 'current' : 'changed';
  }

  /**
   * Adds or updates the `localhost` server in the agent's configuration, pointed at the app's command, and installs the skill. Running it
   * again changes nothing. With `readOnly` the server runs `localhost mcp --read-only`; without, a server that is read-only stays so.
   * Returns the commands it ran (or would run).
   */
  connect(agent, { readOnly = false } = {}) {
    if (!this.isInstalled(agent)) throw problem(`${agent.name} isn’t on this computer: there is no ${display(agent.folder, this.home)}.`);
    const original = this.files.read(agent.config);
    const text = original ?? '';
    const current = this.currentEntry(agent, text);
    const args = readOnly || (current && sameArguments(current.args, READ_ONLY_ARGUMENTS)) ? READ_ONLY_ARGUMENTS : ARGUMENTS;
    const ran = [];
    const claude = agent.slug === 'claude-code' ? this.claudeCommand() : null;
    if (claude) {
      ran.push(...this.connectWithClaude(agent, claude, text, args));
    } else {
      let edited;
      try {
        if (agent.format === 'toml') {
          edited = toml.set(text, TOML_PATH, [['command', toml.renderString(this.command)], ['args', toml.renderStrings(args)]]);
        } else if (current && isStdio(current.value)) {
          // A stdio server of the user's: only what runs it changes, its `env` and the rest stay.
          edited = json.set(json.set(text, [...JSON_PATH, 'command'], this.command), [...JSON_PATH, 'args'], args);
        } else {
          edited = json.set(text, JSON_PATH, this.entry(agent, args));
        }
      } catch (error) {
        throw this.unsupported(agent, error);
      }
      if (original !== edited) {
        this.files.backUp(agent.config, this.backupPath(agent));
        this.files.write(agent.config, edited);
      }
    }
    if (agent.skills) this.installSkill(agent);
    return ran;
  }

  /** Adds or repoints Claude Code's server with `claude mcp`: `add-json` refuses a name that exists, so a server to repoint is removed first. */
  connectWithClaude(agent, claude, text, args) {
    let current;
    try {
      current = json.get(text, JSON_PATH);
    } catch {}
    let desired = this.entry(agent, args);
    if (isStdio(current)) desired = { ...current, command: this.command, args };
    if (JSON.stringify(current) === JSON.stringify(desired)) return [];
    this.files.backUp(agent.config, this.backupPath(agent));
    const commands = [];
    if (current !== undefined) commands.push(['mcp', 'remove', '--scope', 'user', SERVER]);
    commands.push(['mcp', 'add-json', '--scope', 'user', SERVER, JSON.stringify(desired)]);
    for (const command of commands) this.runClaude(claude, command);
    return commands.map((command) => ['claude', ...command]);
  }

  /** Removes the `localhost` server from the agent's configuration, and the skill if it is localhost's. Everything else stays. */
  disconnect(agent) {
    const ran = [];
    const claude = agent.slug === 'claude-code' ? this.claudeCommand() : null;
    const text = this.files.read(agent.config);
    if (claude) {
      let entry;
      try {
        entry = json.get(text ?? '', JSON_PATH);
      } catch {}
      if (entry !== undefined) {
        this.files.backUp(agent.config, this.backupPath(agent));
        const command = ['mcp', 'remove', '--scope', 'user', SERVER];
        this.runClaude(claude, command);
        ran.push(['claude', ...command]);
      }
    } else if (text !== null) {
      let edited;
      try {
        edited = agent.format === 'toml' ? toml.remove(text, TOML_PATH) : json.remove(text, JSON_PATH);
      } catch (error) {
        throw this.unsupported(agent, error);
      }
      if (edited !== undefined) {
        this.files.backUp(agent.config, this.backupPath(agent));
        this.files.write(agent.config, edited);
      }
    }
    if (agent.skills) {
      const skill = skillPath(agent);
      const installed = this.files.read(skill);
      if (installed !== null && isOurSkill(installed)) this.files.removeWithFolder(skill);
      // Cursor read that copy: a connected Cursor gets its own again.
      const cursor = this.get('cursor');
      if (agent.slug !== 'cursor' && this.hasEntry(cursor)) this.installSkill(cursor);
    }
    return ran;
  }

  /** Writes the skill where the agent reads it, unless Cursor reads it already from another agent's folder; keeps one copy Cursor sees. */
  installSkill(agent) {
    const cursorOwn = skillPath(this.get('cursor'));
    const ownIsOurs = () => {
      const text = this.files.read(cursorOwn);
      return text !== null && isOurSkill(text);
    };
    if (agent.slug === 'cursor' && this.sharedSkillForCursor()) {
      if (ownIsOurs()) this.files.removeWithFolder(cursorOwn);
      return;
    }
    const skill = skillPath(agent);
    if (this.files.read(skill) !== this.skill) this.files.write(skill, this.skill);
    // Cursor reads this folder too: its own copy would be a second one.
    if (agent.slug !== 'cursor' && ownIsOurs()) this.files.removeWithFolder(cursorOwn);
  }

  /** The agent whose folder holds this version of the skill, which Cursor reads (https://cursor.com/docs/skills). */
  sharedSkillForCursor() {
    return ['claude-code', 'codex'].map((slug) => this.get(slug)).find((agent) => this.files.read(skillPath(agent)) === this.skill);
  }

  hasEntry(agent) {
    try {
      return this.readEntry(agent) !== undefined;
    } catch {
      return false;
    }
  }

  /** The `localhost` server as configured now, as `{ command, args }`; throws a reason when the file can't be read. */
  readEntry(agent) {
    const text = this.files.read(agent.config);
    if (text === null) return undefined;
    if (agent.format === 'toml') {
      const table = toml.table(text, TOML_PATH);
      return table && { command: table.string('command'), args: table.strings('args') };
    }
    const entry = json.get(text, JSON_PATH);
    if (entry === undefined) return undefined;
    return { command: typeof entry?.command === 'string' ? entry.command : undefined, args: entry?.args };
  }

  /** The entry before connecting, as `{ value, args }`, if the file can be read and has one. */
  currentEntry(agent, text) {
    try {
      if (agent.format === 'toml') {
        const table = toml.table(text, TOML_PATH);
        return table && { value: undefined, args: table.strings('args') };
      }
      const value = json.get(text, JSON_PATH);
      return value === undefined ? undefined : { value, args: value?.args };
    } catch {
      return undefined;
    }
  }

  /** The `localhost` server of a JSON file, fields in the app's order; Claude Desktop's has no `type`. */
  entry(agent, args) {
    return agent.untyped ? { command: this.command, args } : { type: 'stdio', command: this.command, args };
  }

  /** The entry as it reads in the file. */
  entryText(agent, args = ARGUMENTS) {
    if (agent.format === 'toml') return `[mcp_servers.localhost]\ncommand = ${toml.renderString(this.command)}\nargs = ${toml.renderStrings(args)}`;
    return json.set('', JSON_PATH, this.entry(agent, args)).trimEnd();
  }

  backupPath(agent) {
    return path.join(this.context.storageRoot, 'agent-backups', agent.slug, path.basename(agent.config));
  }

  /** The `claude` command to change Claude Code's servers with, if any: on the `PATH`, or where its installers put it. */
  claudeCommand() {
    if (this.context.claude !== undefined) return this.context.claude;
    const name = this.context.platform === 'win32' ? 'claude.exe' : 'claude';
    const folders = (this.context.env.PATH ?? '').split(path.delimiter).filter(Boolean);
    const installed = [path.join(this.home, '.local', 'bin', name), path.join(this.home, '.claude', 'local', name)];
    this.context.claude = [...folders.map((folder) => path.join(folder, name)), ...installed].find(isFile) ?? null;
    return this.context.claude;
  }

  runClaude(claude, args) {
    if (this.files.dryRun) return;
    (this.context.runClaude ?? runClaude)(claude, args);
  }

  unsupported(agent, error) {
    if (!error.reason) return error;
    return problem(`localhost can’t edit ${display(agent.config, this.home)}: ${error.message}. Fix the file, or set up the server by hand.`);
  }
}

function skillPath(agent) {
  return path.join(agent.skills, SKILL_FOLDER, 'SKILL.md');
}

function isStdio(entry) {
  return entry !== null && typeof entry === 'object' && !Array.isArray(entry) && !('url' in entry) && (entry.type === undefined || entry.type === 'stdio');
}

function sameArguments(args, expected) {
  return Array.isArray(args) && args.length === expected.length && args.every((arg, index) => arg === expected[index]);
}

function isOurs(args) {
  return sameArguments(args, ARGUMENTS) || sameArguments(args, READ_ONLY_ARGUMENTS);
}

/** Whether a SKILL.md is localhost's: its front matter names it localhost-mcp. */
export function isOurSkill(text) {
  if (!text.startsWith('---')) return false;
  const front = text.slice(3).split('\n---')[0];
  return front.split('\n').some((line) => line.trim() === `name: ${SKILL_FOLDER}`);
}

function isFile(file) {
  try {
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}

export function problem(message) {
  const error = new Error(message);
  error.reason = true;
  return error;
}

/** Runs `claude` with `args` for up to a minute. Throws with what it printed. */
function runClaude(claude, args) {
  const result = spawnSync(claude, args, { encoding: 'utf8', timeout: 60000, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  const shown = `claude ${args.slice(0, 5).join(' ')}`;
  if (result.error) throw problem(`${shown} failed: ${result.error.message}`);
  if (result.status !== 0) {
    const printed = `${result.stderr}${result.stdout}`.trim().slice(-400);
    throw problem(`${shown} failed: ${printed || `it exited with ${result.status ?? result.signal}`}`);
  }
}
