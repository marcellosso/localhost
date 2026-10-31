/** The /compare page, a /compare/<slug> page per tool, their summary in llms.txt and the README's
 * `## How it compares`. Claims about other tools stay to what their own pages say; `code` is in backticks. */
export interface Alternative {
  /** The anchor on /compare. */
  id: string;
  /** The tool's own page, /compare/<slug>. */
  slug: string;
  name: string;
  /** The tool page's `<title>`, as people search for it, without the site's name. */
  seoTitle: string;
  /** 140–160 characters: the tool page's search snippet. */
  description: string;
  what: string;
  /** Where it is the better choice. */
  wins: string[];
  /** Where localhost fits better. */
  localhost: string[];
  /** One line for the summary table. */
  summary: { wins: string; localhost: string };
}

export const alternatives: Alternative[] = [
  {
    id: 'pm2',
    slug: 'pm2',
    name: 'pm2',
    seoTitle: 'pm2 alternative for local dev servers',
    description: 'pm2 keeps production apps alive on servers. localhost runs the dev servers on your own machine from one window: Dependencies, Ports and one merged log.',
    what: 'A production process manager, mainly for Node.js/Bun apps, driven from the command line.',
    wins: [
      'Keeping apps alive on a server: it restarts them when they crash and starts them again at boot.',
      'Cluster mode, to run one Node.js app on every CPU core.',
      'Headless machines and SSH sessions, where there is no window to open.',
    ],
    localhost: [
      'Development on your own machine: a window per Project, Start All in dependency order, one merged log.',
      'Built for development: your login shell’s `PATH` and version managers, Port readiness and Dependencies between Services, instead of production supervision.',
      'Nothing restarts on its own: a Service that fails stays Failed, with the reason and its output.',
      'Coding agents start, stop, add and edit the same Services over MCP (`localhost mcp`), and you see what they run and change in the window.',
    ],
    summary: { wins: 'Supervising production processes on servers', localhost: 'Dev servers on your own machine, with a GUI' },
  },
  {
    id: 'procfile',
    slug: 'foreman-overmind-honcho',
    name: 'foreman, overmind, honcho',
    seoTitle: 'GUI for Procfile runners: foreman, overmind',
    description: 'foreman, overmind and honcho start a Procfile from one terminal. localhost gives each Service its own start, stop, Port readiness and log, in a window.',
    what: 'Procfile runners: one command starts every process listed in a `Procfile`, with their output in one terminal. Overmind runs each process in tmux, so you can attach to it.',
    wins: [
      'The `Procfile` is committed with the code, so the whole team starts the same processes.',
      'They run anywhere a terminal does, including headless machines and SSH sessions.',
      'The same `Procfile` format Heroku-style platforms read.',
    ],
    localhost: [
      'Unlike foreman and honcho, start, stop or restart one Service from a window (Overmind can restart one process too, from the terminal).',
      'Each Service is Running only once its Port accepts connections, and a taken Port says who holds it.',
      'Nothing is written into your repository; export a Project file when you want to share one.',
    ],
    summary: { wins: 'A shared `Procfile` in the repo, terminal and SSH workflows', localhost: 'Per-Service control, Port readiness and logs in a window' },
  },
  {
    id: 'tmux',
    slug: 'tmux',
    name: 'tmux, tmuxinator',
    seoTitle: 'tmux alternative for running dev servers',
    description: 'tmux keeps terminal sessions alive, on any server. localhost shows every dev server’s state, Port and log in one window, and stops whole process trees.',
    what: 'A terminal multiplexer: panes and windows in one terminal, in sessions that outlive it. tmuxinator describes a project’s panes in a YAML file.',
    wins: [
      'Sessions survive closing the terminal and dropped SSH connections, on any server.',
      'Each process gets a real terminal, so interactive prompts and full-screen tools work. localhost uses pipes, not a terminal.',
      'Everything stays in the terminal, with no app to switch to.',
    ],
    localhost: [
      'Every Service’s state at a glance (Running, Failed with a reason) instead of reading panes.',
      'Stop ends the Service’s whole process tree, every time.',
      'Native on Windows, with Projects on Windows itself or inside WSL.',
    ],
    summary: { wins: 'Remote and headless work, interactive programs', localhost: 'State, Ports and process trees managed for you' },
  },
  {
    id: 'docker-compose',
    slug: 'docker-compose',
    name: 'Docker Compose',
    seoTitle: 'Dev servers without Docker, next to Compose',
    description: 'Docker Compose runs reproducible containers. localhost runs your own commands natively, no Docker required, and runs Compose services right beside them.',
    what: 'Defines and runs a multi-container application from a `compose.yaml`.',
    wins: [
      'Reproducible, isolated environments: the same images on every machine and in CI.',
      'Databases, queues and other dependencies at pinned versions, with nothing installed on the host.',
      'The configuration is committed with the code.',
    ],
    localhost: [
      'Commands that run natively with your own toolchain, through your login shell, without a container.',
      'It runs Compose too: it suggests one Service per Compose service, run as `docker compose up <service>`. Use both together.',
    ],
    summary: { wins: 'Containerized, reproducible stacks', localhost: 'Native commands next to your Compose services' },
  },
  {
    id: 'concurrently',
    slug: 'concurrently',
    name: 'concurrently',
    seoTitle: 'concurrently alternative with a GUI',
    description: 'concurrently starts every dev command in one terminal. localhost starts, stops and restarts each one on its own, in dependency order, with logs kept per Run.',
    what: 'An npm package that runs several commands at once in one terminal, each output line prefixed with its name.',
    wins: [
      'It lives in `package.json`, so everyone on the team gets it with `npm install`.',
      'It works in scripts and CI, where there is no window.',
    ],
    localhost: [
      'Start and stop each Service on its own, and see which one failed and why.',
      'Dependencies wait until a Service is Running before the next one starts.',
      'Logs kept per Run, and Projects that aren’t Node.js.',
    ],
    summary: { wins: 'A one-line `npm run dev` for the whole team, in CI too', localhost: 'Per-Service control, Dependencies and kept logs' },
  },
  {
    id: 'gui-managers',
    slug: 'devglow-devwebui',
    name: 'devglow, DevWebUI',
    seoTitle: 'Open-source devglow and DevWebUI alternative',
    description: 'devglow and DevWebUI are other GUIs for local dev servers. How localhost compares: MIT license, Linux packages, and an MCP server agents install with npx.',
    what: 'Other GUIs for local dev servers. devglow is a paid desktop app ($9.99 lifetime after a 7-day trial) for macOS and Windows. DevWebUI is a control panel served from a local daemon, under a noncommercial license, reading a `.devwebui` file in the repository.',
    wins: [
      'Both have an MCP server for coding agents: devglow’s runs with `npx devglow-mcp`, DevWebUI’s with Bun from its source checkout or as `devwebui mcp`. localhost’s installs the same way: `npx @getlocalhost/cli mcp install` connects Claude Code, Cursor, Codex and Claude Desktop (the app must be installed), and any other stdio MCP client can run `localhost mcp`.',
      'DevWebUI’s `.devwebui` file lives in the repository, so it can be committed and shared with the code.',
    ],
    localhost: [
      'MIT license; Linux packages (devglow has none): `.deb`, `.rpm` and AppImage.',
      'Agents add, edit and remove Projects and Services through the same checks as the app’s editors, and the open app shows each change at once. Removing needs `confirm: true` and never touches a Project’s folder; `localhost mcp --read-only` leaves the editing tools out.',
      'The agent sets no Secrets, reads only those of the Services it starts (and their Dependencies), and its Services stop when it disconnects unless the app is open.',
      'A native SwiftUI app on macOS and one app for Linux and Windows.',
      'Environment Profiles, and Secrets kept in Keychain, Secret Service or Credential Manager.',
    ],
    summary: { wins: 'DevWebUI’s config file in the repo, shared with the code', localhost: 'MIT license; Linux packages; agents connected from the app or `npx @getlocalhost/cli mcp install`, with confirmed deletes and a read-only mode' },
  },
];
