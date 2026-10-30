/** The /agents page's definition and FAQ, also its FAQPage JSON-LD and llms.txt.
 * Answers are plain text; `code` spans are written in backticks. */
export const agentsDefinition =
  'localhost mcp is the Model Context Protocol (MCP) server of localhost app (getlocalhost.app), the free, MIT-licensed desktop process manager for local dev servers on macOS, Windows and Linux. It lets AI coding agents such as Claude Code, Cursor and Codex list, start, stop and inspect the dev servers, databases and workers of your local Projects through localhost instead of a shell, while you see and control the same Services in the localhost app.';

/** The MCP tools, as `localhost mcp` lists them. */
export const tools: { name: string; text: string }[] = [
  { name: 'list_projects', text: 'Every Project: Key, name, folder and Service Keys.' },
  { name: 'list_services', text: 'A Project’s Services as configured: command line, working directory, Port, URL, Dependencies.' },
  { name: 'status', text: 'Each Service’s state, pid, Port, URL and uptime; for a Failed one, why, with its last output.' },
  { name: 'start_service', text: 'Starts one Service after its Dependencies and waits (up to 45 s) until it is Running, Exited or Failed. Optionally on another port, for that Run only.' },
  { name: 'stop_service', text: 'Stops one Service and waits until its whole process tree has ended. Dependents keep running.' },
  { name: 'start_project', text: 'Start All: every Service in dependency order, waiting for each the same way.' },
  { name: 'stop_project', text: 'Stops every running Service of the Project, dependents first.' },
  { name: 'logs', text: 'The latest Run’s output of one Service or the whole Project: the last 200 lines by default (at most 2,000), optionally only recent lines or lines containing some text.' },
  { name: 'ports', text: 'Who listens on which port: one of your Services, a process in a Project’s folder, or something unmanaged.' },
];

export const agentsFaq: { question: string; answer: string[] }[] = [
  {
    question: 'What is localhost mcp?',
    answer: [
      '`localhost mcp` is the MCP server of localhost app, run over stdio (standard input and output) by your coding agent. It gives the agent nine tools (`list_projects`, `list_services`, `status`, `start_service`, `stop_service`, `start_project`, `stop_project`, `logs` and `ports`) over the Projects and Services you configured in the localhost app.',
    ],
  },
  {
    question: 'Which coding agents does it work with?',
    answer: [
      'Any MCP client that starts a server over standard input and output. The app connects Claude Code, Cursor and Codex in one click, in Settings → Agents; Claude Desktop and other clients take the same `command` and `args` in their own configuration file.',
    ],
  },
  {
    question: 'Do the app and the agent see the same Services?',
    answer: [
      'Yes. The app and `localhost mcp` drive one localhost core in the background. A Service the agent starts is Running in the app, with its logs, and one you start in the app is running for the agent. A stop from either stops the same Run, and `status` says who started each one.',
    ],
  },
  {
    question: 'What happens to the Services an agent started when it disconnects?',
    answer: [
      'They stop, with their whole process tree, unless the localhost app is open: then they keep running there, and you stop them when you like. Nothing keeps running unseen.',
    ],
  },
  {
    question: 'Can an agent change my configuration or read my Secrets?',
    answer: [
      'It can’t add or change Projects or Services. Secrets reach only the Services it starts, and their Dependencies; listing, `status`, `logs` and `ports` read none. On macOS, the Keychain asks before `localhost mcp` reads each Secret the first time.',
    ],
  },
  {
    question: 'Do I need to install anything besides the app?',
    answer: [
      'No. The app comes with the `localhost` command, the same version, and updates it with itself. Settings → Agents adds the `localhost` MCP server to Claude Code, Cursor or Codex and installs the localhost-mcp skill, showing exactly what changes first; it can also put `localhost` on your PATH for the terminal.',
    ],
  },
];
