Look where projects keep their commands. From the project's folder:

```sh
npm run                             # lists package.json scripts (pnpm run, yarn run, bun run too)
docker compose config --services    # the Compose services, if there is a compose.yaml
cat Procfile Procfile.dev           # a Procfile runner's processes
ls .nvmrc .tool-versions mise.toml  # which Node, Python or Ruby it expects
cat .env.example                    # the variables it needs
```

Then read the README's "Development" section, if it has one.

## Where does a project keep its dev commands?

Most projects say how they run somewhere in their own files:

- **Node.js and Bun**: the `scripts` in `package.json`. Look for `dev`, `start` or `serve`; in a monorepo, in each package's `package.json` too.
- **Docker Compose**: `compose.yaml` or `docker-compose.yml`, one service per database, queue or app.
- **Procfile**: `Procfile` or `Procfile.dev`, one process per line.
- **Python**: `manage.py runserver` for Django, `fastapi dev` or `uvicorn` for FastAPI, `flask run` for Flask, run with uv, Poetry, Pipenv or a virtualenv.
- **Rust**: `cargo run`, with `--bin <name>` when there are several binaries (in `src/bin/` or `[[bin]]` in `Cargo.toml`).
- **A Makefile or justfile**: `make dev`, `just dev`.

## Why do I keep forgetting how to run it?

Because the answer is spread out. The commands are in one file, the order they start in is in your head, the ports in another file, the variables in a `.env` you didn't commit, and the version of Node in a tool you set up a year ago. Coming back to a project means putting that together again, and so does every coding agent that touches it.

## How can localhost remember how each project runs?

[localhost](/) is a free, open-source desktop app that remembers how each of your projects runs: its commands, ports and environment. You set a project up once; after that, you open localhost, pick the Project and press Start All.

1. **Add the folder** (⌘N, or Ctrl+N). localhost reads it and suggests Services: `package.json` scripts with your package manager, one Service per Compose service, Django, FastAPI and Flask apps, and `cargo run` for each binary. It looks at the folder, the folders up to two levels below it, and workspace packages and Cargo members. Scanning only reads: nothing is written, and nothing goes over the network.
2. **Check the suggestions.** The likely ones are checked, the rest folded away, and you can edit each before anything is saved. Nothing found? Add your own commands: anything your terminal runs.
3. **Write down what you used to remember.** Make the API start after the database, give each Service its Port, and put variables on the Project, the Service or an Environment Profile. Any variable can be a Secret, kept in your system's secret store.

localhost keeps all of it in its own storage, never in your project's folder. To move a Project to another machine, or hand it to a teammate, **Export Project…** writes it as a small YAML [Project file](/docs/project-files), with Secrets listed by name only, and **Import Project…** reads it back after showing what it contains.

## Can I run it from the terminal, or let an agent run it?

Yes. The `localhost` command runs on the same Projects: `localhost projects` lists them, and `localhost up my-saas` runs a Project's Services in the foreground until Ctrl-C ([Command line](/docs/cli)). Coding agents such as Claude Code, Cursor and Codex start the same Services through [`localhost mcp`](/agents), instead of guessing at `npm run dev`.

Start here: [Quick start](/docs/quick-start).
