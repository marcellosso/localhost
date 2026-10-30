The `localhost` command runs on the same core and the same data as the app: list Projects, see what is running, read the latest logs, or run a Project's Services in the foreground until Ctrl-C, or let a coding agent do it.

```sh
localhost projects
localhost status my-saas
localhost logs my-saas web
localhost up my-saas
localhost mcp
```

Projects and Services are named by their **Key**. `localhost mcp` serves them to coding agents; see [Coding agents (MCP)](/docs/mcp).

It comes with the app, the same version. To run it in a terminal, use **Settings → Agents → Use localhost in a terminal → Add to PATH** (see [Use it in a terminal](/docs/mcp#use-it-in-a-terminal)); the Linux `.deb` and `.rpm` put it in `/usr/bin` already, and the [Homebrew cask](/docs/install#homebrew-macos) links it into Homebrew's `bin`. To build it yourself, see [core/README.md](https://github.com/marcellosso/localhost/blob/main/core/README.md#the-localhost-command).
