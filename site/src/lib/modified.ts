import { execFileSync } from 'node:child_process';

const built = new Date().toISOString();

/** When any of `files` (relative to site/) last changed in git, or the build time
 * when git can't tell (no history, or a shallow clone without those commits). */
export function lastModified(files: string[]): string {
  try {
    const date = execFileSync('git', ['log', '-1', '--format=%cI', '--', ...files], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    return date ? new Date(date).toISOString() : built;
  } catch {
    return built;
  }
}
