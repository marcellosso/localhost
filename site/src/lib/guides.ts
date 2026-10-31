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
}

export const guides: Guide[] = [
  {
    slug: 'port-already-in-use',
    title: 'Port 3000 is already in use',
    seoTitle: 'Port 3000 already in use (EADDRINUSE): the fix',
    description: 'Fix “port 3000 is already in use” (EADDRINUSE) on macOS, Linux and Windows: find the process holding the port, stop it, and stop it happening again.',
    summary: 'Find what holds the port with lsof, ss or netstat, stop it, and why the error keeps coming back.',
    label: 'Port already in use',
  },
  {
    slug: 'kill-leftover-dev-servers',
    title: 'Kill leftover dev servers',
    seoTitle: 'Find and kill leftover dev servers',
    description: 'Find and stop dev servers left running by a closed terminal, Claude Code or Cursor: list every listening port, kill the whole process tree, and keep it clean.',
    summary: 'List every process listening on a port, stop the whole tree, not just the parent, and keep it that way.',
    label: 'Leftover dev servers',
  },
  {
    slug: 'claude-code-dev-server',
    title: 'Claude Code started a second dev server',
    seoTitle: 'Claude Code starts a second dev server: fix it',
    description: 'Claude Code starts a second npm run dev, hangs on it, or leaves it running after the chat. Stop the copy, then let it run dev servers through localhost mcp.',
    summary: 'Stop the duplicate, then connect Claude Code to localhost mcp so it starts the Services you already run.',
    label: 'Claude Code dev servers',
  },
  {
    slug: 'run-frontend-and-backend-together',
    title: 'Run frontend and backend together',
    seoTitle: 'Run frontend and backend without terminal tabs',
    description: 'Run the frontend, backend, worker and database of one project without five terminal tabs: concurrently, Procfile runners and tmux, and what localhost adds.',
    summary: 'concurrently, a Procfile runner or tmux in one command, and when a window with Dependencies fits better.',
    label: 'Frontend and backend together',
  },
  {
    slug: 'how-to-run-my-project',
    title: 'How do I run this project again?',
    seoTitle: 'Forgot how to run a project? Find its commands',
    description: 'Forgot how to run a project you haven’t opened in months? Find its dev commands in package.json, Compose files and Cargo, then have localhost remember them.',
    summary: 'Where a project’s commands hide (package.json, Compose, Procfile, Cargo), and how to never look again.',
    label: 'How to run my project',
  },
];
