import type { APIRoute } from 'astro';
import { alternatives } from '../lib/compare';
import { pages } from '../lib/docs';
import { guides } from '../lib/guides';
import { lastModified } from '../lib/modified';

/** Every indexable page, at the address its canonical link names, with the sources it is built from. */
const entries: { path: string; files: string[] }[] = [
  { path: '/', files: ['src/pages/index.astro', 'src/lib/faq.ts', 'src/components/AppWindow.astro'] },
  { path: '/agents', files: ['src/pages/agents.astro', 'src/lib/agents.ts'] },
  { path: '/compare', files: ['src/pages/compare.astro', 'src/lib/compare.ts'] },
  ...alternatives.map((tool) => ({ path: `/compare/${tool.slug}`, files: ['src/pages/compare/[slug].astro', 'src/lib/compare.ts'] })),
  { path: '/guides', files: ['src/pages/guides/index.astro', 'src/lib/guides.ts'] },
  ...guides.map((guide) => ({ path: `/guides/${guide.slug}`, files: [`src/content/guides/${guide.slug}.md`] })),
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
