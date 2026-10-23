/** The one-sentence answer to "what is localhost?", word for word as the README has it. */
export const definition =
  'localhost (getlocalhost.app) is a free, open-source desktop process manager for local development: it starts, stops and watches all your dev servers, workers and databases from one window instead of terminal tabs. Native on macOS, built with Tauri for Windows and Linux. A GUI alternative to pm2, foreman and tmux for local dev.';

/** The homepage FAQ, also its FAQPage JSON-LD, llms.txt and the README's `## FAQ`.
 * Answers are plain text; `code` spans are written in backticks. */
export const faq: { question: string; answer: string[] }[] = [
  {
    question: 'What is localhost app?',
    answer: [
      definition,
      'A Project is a folder; a Service is one long-running command in it, such as `pnpm dev`, `uv run fastapi dev` or `docker compose up postgres`. localhost runs it in your login shell, keeps its output, knows when it is up, and stops its whole process tree when you ask.',
    ],
  },
  {
    question: 'Is localhost a pm2 alternative?',
    answer: [
      'For local development, yes. pm2 is a production process manager, mainly for Node.js/Bun apps, built to keep them alive on servers. localhost is built for the dev servers on your own machine: a window per Project, Dependencies between Services, a Ports view and one merged log. It never restarts a Service on its own. To keep processes running in production, keep pm2.',
    ],
  },
  {
    question: 'How do I run frontend, backend and database together?',
    answer: [
      'Add the Project’s folder and localhost suggests Services from `package.json` scripts, Compose files, Python apps and Cargo binaries. Make the API start after the database, then press Start All (⇧⌘R, or Ctrl+Shift+R on Linux and Windows): each Service waits until its Dependencies are Running, and All services shows every log in one stream.',
    ],
  },
  {
    question: 'Is it free and open source?',
    answer: ['Yes. localhost is free and open source under the MIT license. There is no account and no backend, and it works offline.'],
  },
  {
    question: 'Does it work on Windows/WSL and Linux?',
    answer: [
      'Yes. On Windows 10 or 11 (x64), a Project runs on Windows itself or inside a WSL 2 distro. On Linux there are `.deb` (Ubuntu 22.04+, Debian 12+), `.rpm` (Fedora) and AppImage packages for x86_64. On macOS it needs macOS 15 or later, on Apple silicon or Intel.',
      'Releases aren’t notarized yet, so the first launch on macOS needs one extra step: System Settings → Privacy & Security → Open Anyway, or `xattr -dr com.apple.quarantine /Applications/localhost.app` in Terminal. On Windows, SmartScreen asks once: click More info, then Run anyway.',
    ],
  },
];

/** An answer paragraph as plain text, for JSON-LD and llms.txt. */
export const plain = (text: string) => text.replaceAll('`', '');

/** An answer paragraph as HTML, with its backtick spans as `<code>`. */
export function html(text: string): string {
  const escaped = text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
  return escaped.replace(/`([^`]+)`/g, '<code>$1</code>');
}
