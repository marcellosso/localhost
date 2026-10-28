/** The docs, in reading order. `collection` and `id` name the Markdown entry:
 * `docs` lives in src/content/docs, `reference` is the repository's docs/ folder. */
export interface DocPage {
  slug: string;
  title: string;
  description: string;
  collection: 'docs' | 'reference';
  id: string;
}

export const sections: { title: string; pages: DocPage[] }[] = [
  {
    title: 'Getting started',
    pages: [
      { slug: '', title: 'Introduction', description: 'What localhost is, and the few words it uses.', collection: 'docs', id: 'introduction' },
      { slug: 'install', title: 'Install', description: 'Download localhost for macOS, Linux or Windows, and open it the first time.', collection: 'docs', id: 'install' },
      { slug: 'quick-start', title: 'Quick start', description: 'Add a Project, check its Services and start them.', collection: 'docs', id: 'quick-start' },
    ],
  },
  {
    title: 'Using localhost',
    pages: [
      { slug: 'services', title: 'Services and states', description: 'What a Service is, its six states, Dependencies and stopping.', collection: 'docs', id: 'services' },
      { slug: 'logs', title: 'Logs', description: 'Each Service’s output, the merged view, filtering and what is kept.', collection: 'docs', id: 'logs' },
      { slug: 'environment', title: 'Environment and Secrets', description: 'Variables on Projects, Services and Environment Profiles, and Secrets.', collection: 'docs', id: 'environment' },
      { slug: 'ports', title: 'Ports', description: 'When a Service is up, taken ports, and the Ports view.', collection: 'docs', id: 'ports' },
      { slug: 'keyboard', title: 'Keyboard shortcuts', description: 'The command palette and every shortcut.', collection: 'docs', id: 'keyboard' },
      { slug: 'project-files', title: 'Project files', description: 'The YAML file Export writes and Import reads.', collection: 'reference', id: 'configuration' },
    ],
  },
  {
    title: 'Platforms',
    pages: [
      { slug: 'macos', title: 'macOS', description: 'Keychain prompts for Secrets.', collection: 'docs', id: 'macos' },
      { slug: 'linux', title: 'Linux', description: 'The Secret Service and the tray icon on GNOME.', collection: 'docs', id: 'linux' },
      { slug: 'windows', title: 'Windows and WSL', description: 'Projects on Windows itself or in a WSL distro.', collection: 'docs', id: 'windows' },
    ],
  },
  {
    title: 'Reference',
    pages: [
      { slug: 'cli', title: 'Command line', description: 'The localhost command.', collection: 'docs', id: 'cli' },
      { slug: 'mcp', title: 'Coding agents (MCP)', description: 'Let Cursor, Claude Code and other agents start, stop and inspect your Services.', collection: 'docs', id: 'mcp' },
      { slug: 'data', title: 'Data and storage', description: 'Where configuration, logs and Secrets are kept.', collection: 'docs', id: 'data' },
    ],
  },
];

export const pages = sections.flatMap((section) => section.pages);
