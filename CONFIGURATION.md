# Project file reference

localhost keeps each Project's configuration in its own storage (`~/Library/Application Support/localhost`), never in your project folder. To move a Project to another Mac, keep it in a dotfiles repo or share it with a teammate, export it as a **Project file**: one YAML file per Project.

- **File → Export Project…** writes the selected Project to a file you choose.
- **File → Import Project…** reads one, validates it, and shows what it contains before anything is added.

You can also write a Project file by hand and import it.

## Example

```yaml
localhost: 1
name: My SaaS
key: my-saas
path: ~/Code/my-saas
variables:
  LOG_LEVEL: debug
secrets:
  - SENTRY_DSN
services:
  database:
    name: Database
    command: docker compose up postgres
    cwd: .
    port: 5432
    port_can_change: false
  api:
    name: API
    command: uv run fastapi dev --port $PORT
    cwd: api
    port: 8000
    depends_on:
      - database
    variables:
      DATABASE_URL: postgres://localhost:5432/app
    secrets:
      - STRIPE_KEY
  web:
    name: Web
    command: pnpm dev
    cwd: web
    port: 3000
    url: http://localhost:3000/admin
    start_all: false
    depends_on:
      - api
profiles:
  Development:
    variables:
      NODE_ENV: development
  Testing: {}
active_profile: Development
```

## Fields

### Project

| Field | Required | Value |
|---|---|---|
| `localhost` | yes | The format version: `1`. A file without it isn't a Project file. |
| `name` | yes | The display name. |
| `key` | yes | The Project's Key: lowercase letters and digits in groups separated by single hyphens (`my-saas`), at most 64 characters. Unique among your Projects. |
| `path` | yes | The Project's folder, absolute. `~` stands for your home folder; export writes it that way. |
| `variables` | no | Environment variables for every Service: `NAME: value` lines. |
| `secrets` | no | Names of Secrets for every Service, as a list. No values. |
| `services` | no | The Services, keyed by their Key (below), in the order they appear in the app. |
| `profiles` | no | Environment Profiles, keyed by name (below). |
| `active_profile` | no | The name of the active Profile. It must be one of `profiles`. |

### Service

Each entry under `services` is keyed by the Service's Key (same rules as the Project's; unique within the Project).

| Field | Required | Default | Value |
|---|---|---|---|
| `command` | yes | | The command line, run as `$SHELL -l -c "<command>"` in the working directory. |
| `name` | no | the Key | The display name. |
| `cwd` | no | `.` | The working directory, relative to the Project's `path` (`web`, `packages/api`, `../shared`). `.` is the Project folder. Absolute paths and `~` aren't allowed. |
| `port` | no | none | The Service's Port, 1–65535. The command gets the actual port as `$PORT`, and the Service counts as Running once it accepts connections. |
| `url` | no | none | An `http` or `https` URL for Open in Browser. Its port is replaced by the actual port of the Run. Without one, Open in Browser uses `http://localhost:<port>`. |
| `port_can_change` | no | `true` | `false`: never start this Service on another port when its Port is taken. |
| `start_all` | no | `true` | `false`: Start All skips this Service (shown as **Manual** in the app), unless something being started depends on it. |
| `depends_on` | no | none | Keys of Services of this file that must be Running (or have Exited) before this one starts, as a list. |
| `variables` | no | none | The Service's own environment variables. |
| `secrets` | no | none | Names of the Service's own Secrets. |

### Environment Profile

Each entry under `profiles` is keyed by the Profile's name (any text, unique within the file). It has `variables` and `secrets`, both optional; write `{}` for a Profile with neither.

### Variables and Secrets

On every layer (Project, Service, Profile):

- `variables` is a mapping of names to values. A name is a portable shell name: letters, digits and `_`, not starting with a digit. Values are always text: `PORT_OFFSET: 10` is the text `10`. An empty value (`DEBUG:` with nothing after it) is the empty text.
- `secrets` is a list of names. A Secret's value lives in the Keychain and is **never written to a Project file**. After an import, the review lists the Secrets still to fill in; set them in the Service's editor or in Project → Edit Environment….
- A name can't be both a variable and a Secret on the same layer.

When a Service starts, its environment is built from (later ones win): the login shell's environment, the Project's variables, the Service's, then the active Profile's. `PORT` is set when the Service has a Port.

## The YAML localhost reads

Project files use a small, strict subset of YAML, read by localhost itself:

- block mappings (`key: value`, nested by indentation) and lists of single values (`- item`, or `[a, b]` on one line);
- `{}` for an empty mapping;
- plain, `'single-quoted'` and `"double-quoted"` values (with `\"`, `\\`, `\n`, `\t`, … escapes);
- `#` comments.

Anything else is rejected with its line number rather than skipped: multi-line `|` / `>` values, anchors and aliases, tags, lists of mappings, nested `{…}` / `[…]`.

Unknown fields are errors too, so a typo (`dependson:`) can't silently do nothing. A field defined twice is an error.

## How export writes it

- Fields at their default are left out (`cwd` is always written), and variables and Secret names are sorted, so the same configuration always gives the same file: export, import and export again gives an identical file.
- Values that another YAML reader could take for something else than text (`true`, `no`, `8080`, `~`, a leading `-` or `#`, …) are written in double quotes.
- What isn't exported: Secret values, internal identifiers, the Projects' order in the sidebar, whether a Project is a favourite, the app's settings, and logs.

## Importing

The import review shows the Project's Services, its environment and the Secrets to set, and checks the file with the same rules as the app's editors: Keys, working directories, ports, URLs, Dependencies (a Key no Service has, or a cycle such as `api → web → api`) and the active Profile. An invalid file is rejected with what is wrong and where, for example “Line 27, services › web › depends_on: No service in this file has the key “ap”.”

- **Folder**: prefilled from `path`. If that folder isn't on this Mac, choose where the Project is.
- **Key already used** by one of your Projects: choose
  - **Keep both**: the import gets a free Key (`my-saas-2`); or
  - **Replace**: the existing Project takes the file's configuration and keeps its logs. Services matching by Key and Profiles matching by name keep their logs and stored Secret values; Services the file doesn't have are stopped and removed. The review lists them.

Nothing is ever overwritten without choosing Replace.

## Versions

`localhost: 1` is the only format version so far. A file with a newer version than your localhost knows is refused with a message saying so; update localhost to import it.
