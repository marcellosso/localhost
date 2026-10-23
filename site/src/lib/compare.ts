/** The /compare page, its summary in llms.txt and the README's `## How it compares`.
 * Claims about other tools stay to what their own pages say; `code` is in backticks. */
export interface Alternative {
  id: string;
  name: string;
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
    name: 'pm2',
    what: 'A process manager for Node.js apps in production, driven from the command line.',
    wins: [
      'Keeping apps alive on a server: it restarts them when they crash and starts them again at boot.',
      'Cluster mode, to run one Node.js app on every CPU core.',
      'Headless machines and SSH sessions, where there is no window to open.',
    ],
    localhost: [
      'Development on your own machine: a window per Project, Start All in dependency order, one merged log.',
      'Any command, not only Node.js: `uv run fastapi dev`, `cargo run`, `docker compose up postgres`.',
      'Nothing restarts on its own: a Service that fails stays Failed, with the reason and its output.',
    ],
    summary: { wins: 'Production Node.js processes on servers', localhost: 'Dev servers of any language on your machine, with a GUI' },
  },
  {
    id: 'procfile',
    name: 'foreman, overmind, honcho',
    what: 'Procfile runners: one command starts every process listed in a `Procfile`, with their output in one terminal. Overmind runs each process in tmux, so you can attach to it.',
    wins: [
      'The `Procfile` is committed with the code, so the whole team starts the same processes.',
      'They run anywhere a terminal does, including headless machines and SSH sessions.',
      'The same `Procfile` format Heroku-style platforms read.',
    ],
    localhost: [
      'Start, stop or restart one Service without touching the others.',
      'Each Service is Running only once its Port accepts connections, and a taken Port says who holds it.',
      'Nothing is written into your repository; export a Project file when you want to share one.',
    ],
    summary: { wins: 'A shared `Procfile` in the repo, terminal and SSH workflows', localhost: 'Per-Service control, Port readiness and logs in a window' },
  },
  {
    id: 'tmux',
    name: 'tmux, tmuxinator',
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
    name: 'Docker Compose',
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
    name: 'concurrently',
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
    name: 'devglow, DevWebUI',
    what: 'Other GUIs for local dev servers. devglow is a paid desktop app for macOS and Windows. DevWebUI is a control panel served from a local daemon, under a noncommercial license, reading a `.devwebui` file in the repository.',
    wins: [
      'Both ship an MCP server today, so coding agents such as Claude Code and Cursor can start and stop processes. localhost doesn’t have one yet.',
    ],
    localhost: [
      'MIT licensed and free for any use.',
      'A native SwiftUI app on macOS and one app for Linux and Windows, with `.deb`, `.rpm` and AppImage packages.',
      'Environment Profiles, and Secrets kept in Keychain, Secret Service or Credential Manager.',
    ],
    summary: { wins: 'Agent control over MCP today', localhost: 'MIT license, Linux packages, Environment Profiles and Secrets' },
  },
];
