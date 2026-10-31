/** The docs, in reading order. `collection` and `id` name the Markdown entry:
 * `docs` lives in src/content/docs, `reference` is the repository's docs/ folder. */
export interface DocPage {
  slug: string;
  title: string;
  /** The `<title>` and headline for search, when it should say more than the navigation's `title`. */
  seoTitle?: string;
  /** 140–155 characters: the search snippet, and the line under the page's heading. */
  description: string;
  collection: 'docs' | 'reference';
  id: string;
}

export const sections: { title: string; pages: DocPage[] }[] = [
  {
    title: 'Getting started',
    pages: [
      { slug: '', title: 'Introduction', description: 'What localhost app is: a desktop process manager that starts, stops and watches your local dev servers, and the words it uses, such as Project and Service.', collection: 'docs', id: 'introduction' },
      { slug: 'install', title: 'Install', description: "Download localhost app for macOS (DMG or Homebrew), Linux (.deb, .rpm, AppImage) or Windows, what each needs, and how to open it the first time.", collection: 'docs', id: 'install' },
      { slug: 'quick-start', title: 'Quick start', description: 'Add your first Project to localhost app: scan its folder for Services, check the suggestions, press Start All, then let a coding agent run it too.', collection: 'docs', id: 'quick-start' },
    ],
  },
  {
    title: 'Using localhost',
    pages: [
      { slug: 'services', title: 'Services and states', description: 'What a Service is in localhost app: its command, working directory, Port and Dependencies, its six states, and how a stop ends its whole process tree.', collection: 'docs', id: 'services' },
      { slug: 'logs', title: 'Logs', description: "Each Service's output in localhost app: the merged All services view, filtering, clickable URLs, and how many Runs are kept on disk, up to 10 MB each.", collection: 'docs', id: 'logs' },
      { slug: 'environment', title: 'Environment and Secrets', description: "Variables on Projects, Services and Environment Profiles in localhost app, the order they apply in, and Secrets kept in your system's secret store.", collection: 'docs', id: 'environment' },
      { slug: 'ports', title: 'Ports', description: 'How localhost app knows a Service is up from its Port, what happens when a port is taken, moving a Run to a free port, and the Ports view of listeners.', collection: 'docs', id: 'ports' },
      { slug: 'updates', title: 'Updates', seoTitle: 'How localhost updates on macOS, Windows, Linux', description: 'How localhost app checks for, downloads and installs updates on macOS, Windows and Linux, why running Services stop first, and how to turn checks off.', collection: 'docs', id: 'updates' },
      { slug: 'keyboard', title: 'Keyboard shortcuts', description: 'The command palette and every keyboard shortcut of localhost app, on macOS, Linux and Windows: start, stop, Start All, logs, Projects and Services.', collection: 'docs', id: 'keyboard' },
      { slug: 'project-files', title: 'Project files', description: "The YAML Project file that localhost app's Export writes and Import reads: every field, Secrets listed by name only, and the review before an import.", collection: 'reference', id: 'configuration' },
    ],
  },
  {
    title: 'Platforms',
    pages: [
      { slug: 'macos', title: 'macOS', description: 'localhost app on macOS: a native SwiftUI app with a menu bar companion, what it needs to run, and why the Keychain may ask before a Secret is first used.', collection: 'docs', id: 'macos' },
      { slug: 'linux', title: 'Linux', description: 'localhost app on Linux: updates for the AppImage, .deb and .rpm, the Secret Service it needs for Secrets, the tray icon on GNOME, and coding agents.', collection: 'docs', id: 'linux' },
      { slug: 'windows', title: 'Windows and WSL', description: 'localhost app on Windows: Projects on Windows itself or in a WSL distro, cmd.exe with a fresh environment, coding agents over a named pipe, and updates.', collection: 'docs', id: 'windows' },
    ],
  },
  {
    title: 'Reference',
    pages: [
      { slug: 'cli', title: 'Command line', description: "The localhost command: list Projects, see what is running, read the latest logs and run a Project's Services in the foreground, on the app's own data.", collection: 'docs', id: 'cli' },
      { slug: 'mcp', title: 'Coding agents (MCP)', seoTitle: 'MCP server for AI coding agents', description: "localhost mcp, the MCP server of localhost app: connect Claude Code, Cursor or Codex from the app or with npx, its tools, and how agents share your Services.", collection: 'docs', id: 'mcp' },
      { slug: 'skills', title: 'Agent skill', seoTitle: 'localhost-mcp skill for Claude Code and Codex', description: 'The localhost-mcp agent skill: what it teaches Claude Code, Codex and other agents about running dev servers through localhost app, and how to install it.', collection: 'docs', id: 'skills' },
      { slug: 'data', title: 'Data and storage', description: 'Where localhost app keeps configuration, logs, Run records and Secrets on macOS, Linux and Windows, and how to point it at another folder to try a build.', collection: 'docs', id: 'data' },
    ],
  },
];

export const pages = sections.flatMap((section) => section.pages);
