import type { APIRoute } from 'astro';
import { agentsDefinition, agentsFaq, tools } from '../lib/agents';
import { alternatives } from '../lib/compare';
import { pages } from '../lib/docs';
import { definition, faq, plain } from '../lib/faq';
import { releases, repository } from '../lib/site';

/** A plain-text summary for language models, in the llms.txt format: what localhost is, the FAQ and where to read more. */
export const GET: APIRoute = ({ site }) => {
  const url = (path: string) => new URL(path, site).toString();
  const lines = [
    '# localhost app',
    '',
    `> ${definition}`,
    '',
    '## FAQ',
    '',
    ...faq.flatMap((item) => [`### ${item.question}`, '', ...item.answer.map(plain).flatMap((paragraph) => [paragraph, ''])]),
    '## Coding agents (MCP)',
    '',
    `- [localhost for AI coding agents](${url('/agents')}): ${agentsDefinition}`,
    `- Tools: ${tools.map((tool) => tool.name).join(', ')}.`,
    '',
    ...agentsFaq.flatMap((item) => [`### ${item.question}`, '', ...item.answer.map(plain).flatMap((paragraph) => [paragraph, ''])]),
    '## Docs',
    '',
    ...pages.map((page) => `- [${page.title}](${url(page.slug ? `/docs/${page.slug}` : '/docs')}): ${page.description}`),
    '',
    '## Compare',
    '',
    `- [localhost compared to other tools](${url('/compare')}): where each alternative wins, and where localhost fits better.`,
    ...alternatives.map((tool) => `  - ${tool.name}: ${plain(tool.summary.wins)}. localhost: ${plain(tool.summary.localhost)}.`),
    '',
    '## More',
    '',
    `- [Changelog](${url('/changelog')}): what changed in each release.`,
    `- [Download](${releases}): the latest release on GitHub.`,
    `- [Source code](${repository}): MIT licensed.`,
    '',
  ];
  return new Response(lines.join('\n'), { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
};
