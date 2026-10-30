import { execFileSync } from 'node:child_process';
import type { APIRoute } from 'astro';
import { pages } from '../lib/docs';

const built = new Date().toISOString();

/** When any of `files` (relative to site/) last changed in git, or the build time
 * when git can't tell (no history, or a shallow clone without those commits). */
function lastModified(files: string[]): string {
  try {
    const date = execFileSync('git', ['log', '-1', '--format=%cI', '--', ...files], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    return date ? new Date(date).toISOString() : built;
  } catch {
    return built;
  }
}

/** Every indexable page, at the address its canonical link names, with the sources it is built from. */
const entries: { path: string; files: string[] }[] = [
  { path: '/', files: ['src/pages/index.astro', 'src/lib/faq.ts', 'src/components/AppWindow.astro'] },
  { path: '/agents', files: ['src/pages/agents.astro', 'src/lib/agents.ts'] },
  { path: '/compare', files: ['src/pages/compare.astro', 'src/lib/compare.ts'] },
  { path: '/changelog', files: ['src/pages/changelog.astro', 'src/content/changelog'] },
  ...pages.map((page) => ({
    path: page.slug ? `/docs/${page.slug}` : '/docs',
    files: [page.collection === 'reference' ? `../docs/${page.id}.md` : `src/content/docs/${page.id}.md`],
  })),
];

export const GET: APIRoute = ({ site }) => {
  const urls = entries
    .map(({ path, files }) => `  <url><loc>${new URL(path, site)}</loc><lastmod>${lastModified(files)}</lastmod></url>`)
    .join('\n');
  return new Response(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`, {
    headers: { 'Content-Type': 'application/xml; charset=utf-8' },
  });
};
