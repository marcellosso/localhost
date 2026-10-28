The `localhost` command runs on the same core and the same data as the app: list Projects, see what is running, read the latest logs, or run a Project's Services in the foreground until Ctrl-C, or let a coding agent do it.

```sh
localhost projects
localhost status my-saas
localhost logs my-saas web
localhost up my-saas
localhost mcp
```

Projects and Services are named by their **Key**. `localhost mcp` serves them to coding agents; see [Coding agents (MCP)](/docs/mcp).

It isn't in the release downloads yet; build it from source, see [core/README.md](https://github.com/marcellosso/localhost/blob/main/core/README.md#the-localhost-command).
