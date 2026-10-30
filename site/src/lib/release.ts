import { releases, repository } from './site';

export type Platform = 'macos' | 'windows' | 'linux';

export interface Release {
  /** Without the leading `v`, e.g. `0.3.1`; `<version>` when no release is published. */
  version: string;
  /** When it was published; missing when no release could be read. */
  published?: Date;
  /** Direct download links. Each falls back to the Releases page when the asset is missing. */
  mac: string;
  windows: string;
  deb: string;
  rpm: string;
  appImage: string;
  /** The file names, for the install steps. */
  files: { mac: string; windows: string; deb: string; rpm: string; appImage: string };
}

const patterns = {
  mac: /\.dmg$/,
  windows: /_x64-setup\.exe$/,
  deb: /_amd64\.deb$/,
  rpm: /\.x86_64\.rpm$/,
  appImage: /\.AppImage$/,
} as const;

let cached: Promise<Release> | undefined;

/** The latest published release, read from GitHub once per build. Publishing
 * a release rebuilds the site through Vercel's deploy hook
 * (.github/workflows/site.yml), so the links name that release's files.
 * Without one, or without network, every link goes to the Releases page. */
export function latestRelease(): Promise<Release> {
  cached ??= load();
  return cached;
}

async function load(): Promise<Release> {
  const generic = {
    mac: 'localhost-<version>.dmg',
    windows: 'localhost_<version>_x64-setup.exe',
    deb: 'localhost-app_<version>_amd64.deb',
    rpm: 'localhost-app-<version>-1.x86_64.rpm',
    appImage: 'localhost-app_<version>_amd64.AppImage',
  };
  const fallback: Release = { version: '<version>', mac: releases, windows: releases, deb: releases, rpm: releases, appImage: releases, files: generic };

  const api = repository.replace('https://github.com/', 'https://api.github.com/repos/') + '/releases/latest';
  const token = process.env.GITHUB_TOKEN;
  try {
    const response = await fetch(api, { headers: { Accept: 'application/vnd.github+json', ...(token ? { Authorization: `Bearer ${token}` } : {}) } });
    if (!response.ok) {
      console.warn(`[release] No published release (${response.status}); download links go to the Releases page.`);
      return fallback;
    }
    const data = (await response.json()) as { tag_name: string; published_at: string | null; assets: { name: string; browser_download_url: string }[] };
    const release = { ...fallback, files: { ...generic }, version: data.tag_name.replace(/^v/, ''), published: data.published_at ? new Date(data.published_at) : undefined };
    for (const [key, pattern] of Object.entries(patterns) as [keyof typeof patterns, RegExp][]) {
      const asset = data.assets.find((candidate) => pattern.test(candidate.name));
      if (asset) {
        release[key] = asset.browser_download_url;
        release.files[key] = asset.name;
      }
    }
    return release;
  } catch (error) {
    console.warn(`[release] Couldn't reach GitHub (${(error as Error).message}); download links go to the Releases page.`);
    return fallback;
  }
}
