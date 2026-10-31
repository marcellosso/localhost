The quickest way is one command that starts them all in one terminal, with each line labelled. With [concurrently](https://www.npmjs.com/package/concurrently):

```sh
npx concurrently -n db,api,web \
  "docker compose up postgres" \
  "cd api && uv run fastapi dev" \
  "npm --prefix web run dev"
```

Or with a `Procfile` and a Procfile runner (foreman, overmind or honcho):

```
db: docker compose up postgres
api: cd api && uv run fastapi dev
web: npm --prefix web run dev
worker: cd api && uv run python -m worker
```

```sh
overmind start      # or: foreman start, honcho start
```

## Which tool should I use to run several dev servers at once?

Each one is good at something:

- **concurrently** lives in `package.json`, so `npm run dev` starts everything for the whole team, in CI too. It runs them side by side in one terminal; it doesn't wait for one to be ready before the next, so people add [wait-on](https://www.npmjs.com/package/wait-on) for the database.
- **A Procfile runner** reads a `Procfile` committed with the code, and works on any machine with a terminal, over SSH too. Overmind runs each process in tmux, so you can attach to one or restart it.
- **tmux** gives each process its own pane, a real terminal, in a session that survives closing the window:

  ```sh
  tmux new-session -d -s app 'cd api && uv run fastapi dev'
  tmux split-window -t app 'npm --prefix web run dev'
  tmux attach -t app
  ```

If one of these covers your day, keep it. They are free, small and scriptable.

## Where do one-terminal setups fall short?

- **Everything restarts together.** Changing the API's environment means stopping the frontend too, with most of them.
- **"Started" isn't "ready".** The API starts before Postgres accepts connections, fails once, and you restart it by hand.
- **A failure scrolls away.** Which one crashed, and why, is somewhere up in the shared output.
- **It's one more tab.** Five projects means five of these, each in a tab you have to find again.

## How does localhost run a whole project?

[localhost](/) is a free, open-source desktop app for exactly this. A Project is your folder; each Service is one of the commands above, run in your login shell, so your `PATH` and version managers apply. No Docker is required: a Compose service is just one more Service, such as `docker compose up postgres`.

- **Start All, in order.** Make the API start after the database. **Start All** (⇧⌘R, or Ctrl+Shift+R) starts every Service in dependency order, and each waits until its Dependencies are Running, which, for a Service with a Port, means the port accepts connections.
- **One at a time when you want.** Start, stop or restart a single Service (⌘R, ⌘.) and leave the rest running. A Service marked Manual stays out of Start All until you need it.
- **One log, or each one.** **All services** shows every Service's output in one stream, each line prefixed with its Key. Select one to see only its log, kept per Run.
- **Failures that say why.** A Service that crashes is Failed, with the reason and its last lines, and the Services waiting for it don't start, and say so.
- **Every project in one window.** Each Project keeps its Services, Ports and environment, so the next one is a click away, not another tab.

Set it up in a minute: localhost scans the folder and suggests Services from `package.json` scripts, Compose files, Python apps and Cargo binaries ([Quick start](/docs/quick-start)). For where each tool wins, side by side: [localhost compared to concurrently](/compare/concurrently), [to Procfile runners](/compare/foreman-overmind-honcho), [to tmux](/compare/tmux) and [all comparisons](/compare).
