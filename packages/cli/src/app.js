// The localhost app on this computer: the `localhost` command it comes with,
// which agents run as `localhost mcp`, and how to install the app when it
// isn't there. This package doesn't ship the app or the command.

import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

/** Where to get the app. test/install.test.js checks they match the site's (site/src/lib/site.ts). */
export const website = 'https://www.getlocalhost.app';
export const installDocs = `${website}/docs/install`;
export const releases = 'https://github.com/marcellosso/localhost/releases/latest';
export const brewInstall = 'brew install --cask marcellosso/tap/localhost';

const BUNDLE_ID = 'io.github.marcellosso.localhost';

/** localhost's data folder, as the core and the apps find it (core/localhost-core/src/paths.rs). */
export function storageRoot({ platform, home, env }) {
  const paths = platform === 'win32' ? path.win32 : path.posix;
  if (env.LOCALHOST_STORAGE_ROOT && paths.isAbsolute(env.LOCALHOST_STORAGE_ROOT)) return env.LOCALHOST_STORAGE_ROOT;
  if (platform === 'darwin') return paths.join(home, 'Library', 'Application Support', 'localhost');
  if (platform === 'win32') {
    const data = env.APPDATA && paths.isAbsolute(env.APPDATA) ? env.APPDATA : paths.join(home, 'AppData', 'Roaming');
    return paths.join(data, 'localhost');
  }
  const data = env.XDG_DATA_HOME && paths.isAbsolute(env.XDG_DATA_HOME) ? env.XDG_DATA_HOME : paths.join(home, '.local', 'share');
  return paths.join(data, 'localhost');
}

/**
 * Where each app puts its `localhost` command, in the order they are looked for: inside localhost.app on macOS (in Applications, then
 * wherever Spotlight finds it), beside the app in the Windows install folder, and on Linux `/usr/bin` (.deb, .rpm) or the copy the AppImage
 * makes in localhost's data folder when it runs.
 */
export function* candidates({ platform, home, env, spotlight = findWithSpotlight }) {
  if (platform === 'darwin') {
    const helper = (app) => path.posix.join(app, 'Contents', 'Helpers', 'localhost');
    const apps = ['/Applications/localhost.app', path.posix.join(home, 'Applications', 'localhost.app')];
    yield* apps.map(helper);
    // Asked only when the app isn't in Applications. A copy on a disk image, or translocated by Gatekeeper, goes away: agents couldn't start it later.
    for (const app of spotlight()) {
      if (!apps.includes(app) && !app.startsWith('/Volumes/') && !app.includes('/AppTranslocation/')) yield helper(app);
    }
    return;
  }
  if (platform === 'win32') {
    const local = env.LOCALAPPDATA && path.win32.isAbsolute(env.LOCALAPPDATA) ? env.LOCALAPPDATA : path.win32.join(home, 'AppData', 'Local');
    yield path.win32.join(local, 'localhost', 'localhost.exe');
    return;
  }
  yield '/usr/bin/localhost';
  yield path.posix.join(storageRoot({ platform, home, env }), 'bin', 'localhost');
}

function findWithSpotlight() {
  try {
    const output = execFileSync('mdfind', [`kMDItemCFBundleIdentifier == '${BUNDLE_ID}'`], { encoding: 'utf8', timeout: 5000, stdio: ['ignore', 'pipe', 'ignore'] });
    return output.split('\n').filter((line) => line.endsWith('.app'));
  } catch {
    return [];
  }
}

/** Runs the command with `args`; its output, or `{ error }`. */
export function probe(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8', timeout: 10000, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  if (result.error) return { error: result.error.message };
  if (result.status !== 0) return { error: (result.stderr || result.stdout || `it exited with ${result.status ?? result.signal}`).trim() };
  return { output: result.stdout };
}

/**
 * The app's command: `{ command, version, readOnly }`, `readOnly` saying whether it has `localhost mcp --read-only`. `{ missing: true }`
 * when there is no app; `{ broken: path, error }` when its command is there but doesn't run.
 */
export function findApp(context) {
  const run = context.probe ?? probe;
  let broken;
  for (const command of (context.candidates ?? candidates)(context)) {
    if (!isFile(command)) continue;
    const version = run(command, ['--version']);
    const match = /^localhost (\S+)/.exec(version.output ?? '');
    if (match) {
      const help = run(command, ['--help']);
      return { command, version: match[1], readOnly: (help.output ?? '').includes('--read-only') };
    }
    // Another program called localhost, e.g. in /usr/bin, isn't the app's.
    if (version.error && !broken) broken = { broken: command, error: version.error };
  }
  return broken ?? { missing: true };
}

function isFile(file) {
  try {
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}

/** What to tell someone without the app, for their OS. Homebrew is only for macOS. */
export function installInstructions(platform) {
  if (platform === 'darwin') {
    return [
      'The localhost app isn’t installed: there is no /Applications/localhost.app.',
      'Install it with Homebrew:',
      '',
      `  ${brewInstall}`,
      '',
      `or download the .dmg from ${website} (or ${releases}) and drag localhost to Applications.`,
      'Then run this again.',
    ];
  }
  if (platform === 'win32') {
    return [
      'The localhost app isn’t installed: there is no %LOCALAPPDATA%\\localhost\\localhost.exe.',
      `Download the installer, localhost_<version>_x64-setup.exe, from ${website} (or ${releases}) and run it.`,
      'Then run this again.',
    ];
  }
  return [
    'The localhost app isn’t installed: there is no /usr/bin/localhost.',
    `Download it from ${website} (or ${releases}):`,
    '',
    '  Ubuntu, Debian: sudo apt install ./localhost-app_<version>_amd64.deb',
    '  Fedora:         sudo dnf install ./localhost-app-<version>-1.x86_64.rpm',
    '  Any distro:     the AppImage: chmod +x it and open it once, so it copies its localhost command out',
    '',
    'Then run this again.',
  ];
}

/** What to tell someone whose app's command is there but doesn't run. */
export function brokenInstructions(platform, command, error) {
  const lines = [`The localhost app’s command, ${command}, didn’t run: ${error}`];
  if (platform === 'darwin') lines.push(`Open localhost once from Applications (see ${installDocs}#first-launch), then run this again.`);
  else lines.push(`Open localhost once, or reinstall it from ${website}, then run this again.`);
  return lines;
}
