// @ts-check
import { defineConfig } from 'astro/config';
import { satteri } from '@astrojs/markdown-satteri';

/** Every page's title comes from the docs navigation, so a Markdown file's own
 * `# Title` (docs/configuration.md has one, for GitHub) is dropped. */
const dropTitle = {
  name: 'drop-title',
  /** @param {any} node @param {any} ctx */
  heading(node, ctx) {
    if (node.depth === 1) ctx.removeNode(node);
  },
};

export default defineConfig({
  // The public address: canonical links, the sitemap and social cards are built from it.
  site: 'https://www.getlocalhost.app',
  // Pages are written as `docs/install.html` and linked as `/docs/install`, so
  // each page has one address. Vercel serves that only with `cleanUrls`
  // (vercel.json), which also redirects `/docs/install.html` to it.
  trailingSlash: 'never',
  build: { format: 'file' },
  markdown: {
    processor: satteri({ mdastPlugins: [dropTitle] }),
    shikiConfig: {
      themes: { light: 'github-light', dark: 'github-dark' },
      defaultColor: false,
    },
  },
});
// CI filter test, not for merging
// second site-only change
