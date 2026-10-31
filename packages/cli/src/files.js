// Reading and replacing agents' files without losing what is in them, as the
// core does (core/localhost-core/src/agents/files.rs). A dry run reads
// through the changes it would make and writes none of them.

import fs from 'node:fs';
import path from 'node:path';

export class Files {
  /** @param {{ dryRun?: boolean }} options */
  constructor({ dryRun = false } = {}) {
    this.dryRun = dryRun;
    /** Each changed file: its text before (`null`: there was none) and after (`null`: removed). */
    this.changes = new Map();
    /** Every change in order, as `{ file, before, after }`. */
    this.log = [];
  }

  /** The file's text, or `null` when there is none. */
  read(file) {
    if (this.changes.has(file)) return this.changes.get(file).after;
    try {
      return fs.readFileSync(file, 'utf8');
    } catch (error) {
      if (error.code === 'ENOENT' || error.code === 'ENOTDIR') return null;
      throw failure(file, error);
    }
  }

  exists(file) {
    return this.changes.has(file) ? this.changes.get(file).after !== null : fs.existsSync(file);
  }

  /**
   * Replaces `file` with `text` in one step, so the agent never reads half a file. A symlink (a dotfiles repository) is followed, not
   * replaced, and the file keeps its permissions.
   */
  write(file, text) {
    this.record(file, text);
    if (this.dryRun) return;
    let target = file;
    try {
      target = fs.realpathSync(file);
    } catch {}
    const folder = path.dirname(target);
    const temporary = path.join(folder, `.${path.basename(target)}.localhost-${process.pid}`);
    try {
      fs.mkdirSync(folder, { recursive: true });
      fs.writeFileSync(temporary, text);
      try {
        fs.chmodSync(temporary, fs.statSync(target).mode);
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
      }
      fs.renameSync(temporary, target);
    } catch (error) {
      fs.rmSync(temporary, { force: true });
      throw failure(file, error);
    }
  }

  /** Copies `file` to `backup` unless a backup is there already: the first one is the file as it was before localhost ever changed it. */
  backUp(file, backup) {
    if (this.dryRun || fs.existsSync(backup) || !fs.existsSync(file)) return;
    try {
      fs.mkdirSync(path.dirname(backup), { recursive: true });
      fs.copyFileSync(file, backup);
    } catch (error) {
      throw failure(backup, error);
    }
  }

  /** Removes `file`, then its folder if that is left empty. */
  removeWithFolder(file) {
    this.record(file, null);
    if (this.dryRun) return;
    try {
      fs.rmSync(file, { force: true });
      const folder = path.dirname(file);
      if (fs.readdirSync(folder).length === 0) fs.rmdirSync(folder);
    } catch (error) {
      if (error.code !== 'ENOENT') throw failure(file, error);
    }
  }

  record(file, after) {
    const current = this.read(file);
    const before = this.changes.has(file) ? this.changes.get(file).before : current;
    this.changes.set(file, { before, after });
    this.log.push({ file, before: current, after });
  }
}

export function failure(file, error) {
  const wrapped = new Error(`localhost couldn’t change ${file}: ${error.message}`);
  wrapped.reason = true;
  return wrapped;
}

/** `file` for people: under the home folder as `~/…`. */
export function display(file, home) {
  const relative = path.relative(home, file);
  if (relative === '' || relative.startsWith('..') || path.isAbsolute(relative)) return file;
  return `~${path.sep}${relative}`;
}
