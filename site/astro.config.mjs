// @ts-check
import { defineConfig } from 'astro/config';
import { satteri } from '@astrojs/markdown-satteri';

const base = '/localhost';

/** Every page's title comes from the docs navigation, so a Markdown file's own
 * `# Title` (docs/configuration.md has one, for GitHub) is dropped. */
const dropTitle = {
  name: 'drop-title',
  /** @param {any} node @param {any} ctx */
  heading(node, ctx) {
    if (node.depth === 1) ctx.removeNode(node);
  },
};

/** Markdown links to the site (`/docs/install`) are written without the base
 * the site is deployed under, so they read the same wherever it moves. */
const prefixSiteLinks = {
  name: 'prefix-site-links',
  /** @param {any} node @param {any} ctx */
  link(node, ctx) {
    if (node.url.startsWith('/') && !node.url.startsWith('//')) ctx.setProperty(node, 'url', base + node.url);
  },
};

export default defineConfig({
  site: 'https://marcellosso.github.io',
  base,
  trailingSlash: 'ignore',
  markdown: {
    processor: satteri({ mdastPlugins: [dropTitle, prefixSiteLinks] }),
    shikiConfig: {
      themes: { light: 'github-light', dark: 'github-dark' },
      defaultColor: false,
    },
  },
});
