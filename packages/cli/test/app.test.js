// Where the app's command is looked for on each OS, and that the install
// steps match the site's install dialog.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { test } from 'node:test';

import { brewInstall, candidates, installDocs, installInstructions, releases, storageRoot, website } from '../src/app.js';

test('looks for the command where each app puts it', () => {
  const spotlight = () => ['/Applications/localhost.app', '/Users/me/Downloads/localhost.app', '/Volumes/localhost/localhost.app', '/private/var/folders/x/AppTranslocation/y/d/localhost.app'];
  assert.deepEqual([...candidates({ platform: 'darwin', home: '/Users/me', env: {}, spotlight })], [
    '/Applications/localhost.app/Contents/Helpers/localhost',
    '/Users/me/Applications/localhost.app/Contents/Helpers/localhost',
    '/Users/me/Downloads/localhost.app/Contents/Helpers/localhost',
  ]);
  assert.deepEqual([...candidates({ platform: 'win32', home: 'C:\\Users\\me', env: { LOCALAPPDATA: 'D:\\Local' } })], ['D:\\Local\\localhost\\localhost.exe']);
  assert.deepEqual([...candidates({ platform: 'win32', home: 'C:\\Users\\me', env: {} })], ['C:\\Users\\me\\AppData\\Local\\localhost\\localhost.exe']);
  assert.deepEqual([...candidates({ platform: 'linux', home: '/home/me', env: {} })], ['/usr/bin/localhost', '/home/me/.local/share/localhost/bin/localhost']);
  assert.deepEqual([...candidates({ platform: 'linux', home: '/home/me', env: { XDG_DATA_HOME: '/data' } })], ['/usr/bin/localhost', '/data/localhost/bin/localhost']);
});

test('Spotlight is asked only when the app isn’t in Applications', () => {
  let asked = false;
  const found = candidates({ platform: 'darwin', home: '/Users/me', env: {}, spotlight: () => ((asked = true), []) });
  found.next();
  assert.equal(asked, false);
  [...found];
  assert.equal(asked, true);
});

test('finds localhost’s data folder as the core does', () => {
  assert.equal(storageRoot({ platform: 'darwin', home: '/Users/me', env: {} }), '/Users/me/Library/Application Support/localhost');
  assert.equal(storageRoot({ platform: 'win32', home: 'C:\\Users\\me', env: { APPDATA: 'C:\\Users\\me\\AppData\\Roaming' } }), 'C:\\Users\\me\\AppData\\Roaming\\localhost');
  assert.equal(storageRoot({ platform: 'linux', home: '/home/me', env: { XDG_DATA_HOME: 'relative' } }), '/home/me/.local/share/localhost');
  assert.equal(storageRoot({ platform: 'linux', home: '/home/me', env: { LOCALHOST_STORAGE_ROOT: '/tmp/dev' } }), '/tmp/dev');
});

test('suggests Homebrew only on macOS', () => {
  assert.ok(installInstructions('darwin').some((line) => line.includes(brewInstall)));
  for (const platform of ['win32', 'linux']) assert.ok(!installInstructions(platform).join('\n').includes('brew'), platform);
});

test('the install links and commands are the site’s', () => {
  const site = fs.readFileSync(new URL('../../../site/src/lib/site.ts', import.meta.url), 'utf8');
  assert.ok(site.includes(`export const brewInstall = '${brewInstall}';`), 'site.ts has another brewInstall');
  assert.ok(site.includes("export const repository = 'https://github.com/marcellosso/localhost';"));
  assert.ok(site.includes('export const releases = `${repository}/releases/latest`;'));
  assert.equal(releases, 'https://github.com/marcellosso/localhost/releases/latest');
  const astro = fs.readFileSync(new URL('../../../site/astro.config.mjs', import.meta.url), 'utf8');
  assert.ok(astro.includes(`site: '${website}'`), 'astro.config.mjs has another site');
  assert.ok(fs.existsSync(new URL('../../../site/src/content/docs/install.md', import.meta.url)), installDocs);
  const dialog = fs.readFileSync(new URL('../../../site/src/components/InstallDialog.astro', import.meta.url), 'utf8');
  assert.ok(dialog.includes('sudo apt install ./') && dialog.includes('sudo dnf install ./'));
  const release = fs.readFileSync(new URL('../../../site/src/lib/release.ts', import.meta.url), 'utf8');
  for (const file of ['localhost-app_<version>_amd64.deb', 'localhost-app-<version>-1.x86_64.rpm', 'localhost_<version>_x64-setup.exe']) assert.ok(release.includes(`'${file}'`), file);
  assert.ok(installInstructions('win32').join('\n').includes('localhost_<version>_x64-setup.exe'));
  const linux = installInstructions('linux').join('\n');
  assert.ok(linux.includes('sudo apt install ./localhost-app_<version>_amd64.deb') && linux.includes('sudo dnf install ./localhost-app-<version>-1.x86_64.rpm'));
});

test('the skill it installs is the repository’s', async () => {
  const { readSkill } = await import('../src/cli.js');
  assert.equal(readSkill(), fs.readFileSync(new URL('../../../skills/localhost-mcp/SKILL.md', import.meta.url), 'utf8'));
});
