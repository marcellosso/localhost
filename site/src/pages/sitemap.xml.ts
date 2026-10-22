import type { APIRoute } from 'astro';
import { pages } from '../lib/docs';

/** Every indexable page, at the address its canonical link names. */
export const GET: APIRoute = ({ site }) => {
  const paths = ['/', '/changelog', ...pages.map((page) => (page.slug ? `/docs/${page.slug}` : '/docs'))];
  const urls = paths.map((path) => `  <url><loc>${new URL(path, site)}</loc></url>`).join('\n');
  return new Response(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`, {
    headers: { 'Content-Type': 'application/xml; charset=utf-8' },
  });
};
