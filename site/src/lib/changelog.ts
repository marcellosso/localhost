import { getCollection, type CollectionEntry } from 'astro:content';
import { latestRelease } from './release';

export interface ChangelogEntry {
  entry: CollectionEntry<'changelog'>;
  version: string;
  title: string;
  date: Date;
  released: boolean;
}

/** `0.10.0` → `[0, 10, 0]`; undefined for anything that isn't a plain version. */
function parse(version: string): number[] | undefined {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(version.trim());
  return match ? match.slice(1).map(Number) : undefined;
}

function compare(a: number[], b: number[]): number {
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] - b[i];
  return 0;
}

/** The changelog, newest first. An entry counts as released once GitHub's
 * latest release is at or past its version, so publishing a release is enough
 * and `released` in the frontmatter doesn't have to be flipped by hand; the
 * published release takes GitHub's date. Without GitHub, the frontmatter decides. */
export async function changelog(): Promise<ChangelogEntry[]> {
  const release = await latestRelease();
  const latest = parse(release.version);
  return (await getCollection('changelog'))
    .map((entry) => {
      const version = parse(entry.data.version);
      const published = latest !== undefined && version !== undefined && compare(version, latest) <= 0;
      const isLatest = latest !== undefined && version !== undefined && compare(version, latest) === 0;
      return {
        entry,
        version: entry.data.version,
        title: entry.data.title,
        date: isLatest && release.published ? release.published : entry.data.date,
        released: entry.data.released || published,
      };
    })
    .sort((a, b) => b.date.getTime() - a.date.getTime());
}
