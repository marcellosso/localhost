/** The guides, in the index's order: one problem each, named the way people search for it.
 * Each opens with the command that fixes it today, then how localhost keeps it fixed.
 * `slug` names the Markdown file in src/content/guides. */
export interface Guide {
  slug: string;
  /** The heading, phrased as the problem. */
  title: string;
  /** The `<title>` for search, without the site's name. */
  seoTitle: string;
  /** 140–160 characters: the search snippet, and the line under the page's heading. */
  description: string;
  /** One line for the index, the footer and llms.txt. */
  summary: string;
  /** The footer's short label. */
  label: string;
  /** Shown at the end of the guide and written as its FAQPage JSON-LD. Plain text; `code` in backticks. */
  faq: { question: string; answer: string[] }[];
}

export const guides: Guide[] = [
  {
    slug: 'port-already-in-use',
    title: 'Port 3000 is already in use',
    seoTitle: 'Port 3000 already in use (EADDRINUSE): the fix',
    description: 'Fix “port 3000 is already in use” (EADDRINUSE) on macOS, Linux and Windows: find the process holding the port, stop it, and stop it happening again.',
    summary: 'Find what holds the port with lsof, ss or netstat, stop it, and why the error keeps coming back.',
    label: 'Port already in use',
    faq: [
      {
        question: 'How do I free port 3000 in one command on a Mac?',
        answer: ['`kill $(lsof -ti tcp:3000 -sTCP:LISTEN)` stops whatever listens on port 3000: `-t` prints only the PID. Leave out `-sTCP:LISTEN` and it also matches clients connected to the port, such as your browser.'],
      },
      {
        question: 'Can I just start my dev server on another port?',
        answer: ['Yes: most read `PORT` or take a flag, such as `next dev -p 3001` or `vite --port 5174`. It works for one server, but URLs, proxies and CORS settings that name the old port break until you change them too.'],
      },
      {
        question: 'Does localhost change my command to use another port?',
        answer: ['No. localhost never rewrites command lines. The port reaches the command only as the `PORT` variable (`$PORT`, or `%PORT%` in cmd), so the command has to read it, for example `next dev -p $PORT`. A moved port lasts for that Run only; the configured Port stays.'],
      },
    ],
  },
  {
    slug: 'kill-leftover-dev-servers',
    title: 'Kill leftover dev servers',
    seoTitle: 'Find and kill leftover dev servers',
    description: 'Find and stop dev servers left running by a closed terminal, Claude Code or Cursor: list every listening port, kill the whole process tree, and keep it clean.',
    summary: 'List every process listening on a port, stop the whole tree, not just the parent, and keep it that way.',
    label: 'Leftover dev servers',
    faq: [
      {
        question: 'How do I kill every Node.js process?',
        answer: ['`pkill node` on macOS and Linux, `taskkill /IM node.exe /F` on Windows. It stops every Node.js process, including your editor’s tools and other projects, so stopping by port or PID is safer.'],
      },
      {
        question: 'What is the difference between SIGTERM and SIGKILL?',
        answer: ['SIGTERM asks a process to exit and lets it clean up; SIGKILL ends it at once and can’t be caught. Send SIGTERM first. localhost sends SIGINT, then SIGTERM after 5 seconds, then SIGKILL after 3 more.'],
      },
      {
        question: 'Does localhost stop processes it didn’t start?',
        answer: ['Only when you ask, from the Ports view, and it asks first. It never takes them over as Services, and won’t stop itself, launchd jobs, Windows services or Docker’s backend.'],
      },
      {
        question: 'What happens to my dev servers if localhost crashes?',
        answer: ['localhost records each running Service on disk. At the next launch it lists the ones still running and offers to stop them (the default) or leave them. They are never taken back as Running.'],
      },
    ],
  },
  {
    slug: 'claude-code-dev-server',
    title: 'Claude Code started a second dev server',
    seoTitle: 'Claude Code starts a second dev server: fix it',
    description: 'Claude Code starts a second npm run dev, hangs on it, or leaves it running after the chat. Stop the copy, then let it run dev servers through localhost mcp.',
    summary: 'Stop the duplicate, then connect Claude Code to localhost mcp so it starts the Services you already run.',
    label: 'Claude Code dev servers',
    faq: [
      {
        question: 'Does the localhost app have to be open for Claude Code to use it?',
        answer: ['It has to be installed, not open: `localhost mcp` starts localhost’s background core when it needs it. The Services the agent starts stop when it disconnects, unless the app is open, where they keep running until you stop them.'],
      },
      {
        question: 'How do I check that Claude Code is connected?',
        answer: ['Run `npx @getlocalhost/cli mcp status`, or `claude mcp list`, or type `/mcp` in a Claude Code session: a server named `localhost` should be listed and connected.'],
      },
      {
        question: 'Does this work with Cursor and Codex too?',
        answer: ['Yes. `npx @getlocalhost/cli mcp install` connects Claude Code, Cursor, Codex and Claude Desktop, whichever are installed, and any other MCP client that starts servers over standard input and output can run `localhost mcp`.'],
      },
      {
        question: 'Can the agent change my Projects?',
        answer: ['It can add, edit and remove Projects and Services, checked as the app checks them; removing needs `confirm: true` and never touches a Project’s folder. To leave those tools out, connect with `npx @getlocalhost/cli mcp install --read-only`.'],
      },
    ],
  },
  {
    slug: 'run-frontend-and-backend-together',
    title: 'Run frontend and backend together',
    seoTitle: 'Run frontend and backend without terminal tabs',
    description: 'Run the frontend, backend, worker and database of one project without five terminal tabs: concurrently, Procfile runners and tmux, and what localhost adds.',
    summary: 'concurrently, a Procfile runner or tmux in one command, and when a window with Dependencies fits better.',
    label: 'Frontend and backend together',
    faq: [
      {
        question: 'How do I make the backend wait for the database?',
        answer: ['With concurrently, prefix the command with wait-on: `npx wait-on tcp:5432 && uv run fastapi dev`. In localhost, make the API start after the database: it waits until the database is Running, which for a Service with a Port means the port accepts connections.'],
      },
      {
        question: 'Can I restart one service without stopping the others?',
        answer: ['Not with concurrently, which runs them as one command. Overmind can restart one process (`overmind restart web`). In localhost, ⌘R restarts the selected Service and ⌘. stops it; the others, and the Services that depend on it, keep running.'],
      },
      {
        question: 'Does localhost replace my Procfile or package.json scripts?',
        answer: ['No. It runs them: a Service’s command can be `npm run dev`, `overmind start` or anything else your terminal runs. It writes nothing into your repository, so the team keeps its own setup.'],
      },
    ],
  },
  {
    slug: 'how-to-run-my-project',
    title: 'How do I run this project again?',
    seoTitle: 'Forgot how to run a project? Find its commands',
    description: 'Forgot how to run a project you haven’t opened in months? Find its dev commands in package.json, Compose files and Cargo, then have localhost remember them.',
    summary: 'Where a project’s commands hide (package.json, Compose, Procfile, Cargo), and how to never look again.',
    label: 'How to run my project',
    faq: [
      {
        question: 'Does localhost write a config file into my project?',
        answer: ['No. A Project’s configuration lives in localhost’s own storage, never in its folder. A YAML Project file is written only when you export one.'],
      },
      {
        question: 'What if localhost doesn’t detect my project’s commands?',
        answer: ['Add the Project anyway and add Services yourself (⇧⌘N, or Ctrl+Shift+N): any command line your terminal runs. Detect Services… scans the folder again later.'],
      },
      {
        question: 'How do I share the setup with my team?',
        answer: ['Export Project… writes a YAML Project file, with Secrets listed by name only. A teammate imports it, sees everything it contains before it is added, and fills in the Secrets.'],
      },
      {
        question: 'Which version of Node or Python does localhost use?',
        answer: ['Whichever your terminal uses: commands run in your login shell, so nvm, asdf, mise, pyenv and Homebrew apply. On Windows they run in cmd.exe with your user environment, read fresh at every start.'],
      },
    ],
  },
];
