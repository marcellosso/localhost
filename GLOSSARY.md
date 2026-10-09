# localhost

A native macOS app that remembers how each local development project runs and lets the developer (and later, their agents) start, stop, and inspect it from one place.

## Language

### Configuration

**Project**:
One local development project, rooted at a folder on the developer's machine, that groups the Services needed to work on it.
_Avoid_: Stack, workspace, app, repo

**Service**:
One independently manageable long-running command belonging to a Project, defined by a working directory, a command line, and an environment. A Service is never inherently "a Node service" or "a Docker service".
_Avoid_: Process, task, job, script

**Compose service**:
A service as declared inside a Docker Compose file. A single localhost Service may run one or many Compose services.
_Avoid_: Calling these plain "services"

**Key**:
The short, human-readable, stable handle of a Project or Service (e.g. `my-saas`, `api`), used wherever something must refer to it by name. Project keys are unique across the app; Service keys are unique within their Project.
_Avoid_: Slug, name (the name is the free-form display label)

**Project Configuration**:
Everything localhost remembers about a Project and its Services. Owned and stored by localhost, never written into the Project's folder.
_Avoid_: Config file, manifest

**Dependency**:
A Service that another Service in the same Project needs to be Running, or to have Exited, before it can start.
_Avoid_: Requirement, prerequisite

**Environment Profile**:
A named set of environment variables for a Project (e.g. Development, Testing) that applies to all of its Services; at most one is active per Project at a time.
_Avoid_: Env, mode, stage

**Secret**:
An environment variable whose value is hidden by default and never leaves localhost's protected storage unless explicitly revealed. Any variable — on a Project, a Service, or an Environment Profile — can be a Secret.
_Avoid_: Credential, sensitive var

**Port**:
The one TCP port a Service is expected to listen on, optionally configured. It decides when the Service counts as Running and what its URL points to. Other ports the Service happens to open are not its Port.
_Avoid_: Listening port (for the configured one)

**Suggestion**:
A Service proposed by scanning a Project's folder (its `package.json` scripts, Compose services, Python app, Cargo binaries, …). Suggestions are shown in a review step where the user checks, unchecks and edits them; only the checked ones become Services, and nothing is ever applied silently.
_Avoid_: Detected service, auto-configuration

### Execution

**Run**:
One execution of a Service, from launch until its process tree has fully ended. Carries its own logs, start time, and exit outcome. A Service has at most one active Run.
_Avoid_: Process, session, instance

**Orphan**:
A process tree left over from a Run whose owning localhost session ended unexpectedly (crash, force-quit).
_Avoid_: Zombie, leftover

**Listener**:
A process tree listening on one or more TCP ports on this machine, whoever started it. Each Listener is attributed to a Run (localhost's own), to a Project (its working directory is inside the Project's folder, but localhost didn't start it), or to neither.
_Avoid_: Process, port owner

**Unmanaged Listener**:
A Listener that belongs to no Run and no Project. localhost can show and stop it, but never turns it into a Running Service.
_Avoid_: Rogue, foreign, stray process

**Service State**:
The single, unambiguous status of a Service: Stopped, Starting, Running, Stopping, Exited, or Failed.

**Stopped**:
Not running, either because it was never started or because someone deliberately stopped it — regardless of the exit code the stop produced.

**Starting**:
A start was requested but the Service is not yet considered up: it is waiting for its Dependencies, or its Run has been launched and has not yet passed Starting.

**Running**:
A Run is alive and has passed Starting.

**Stopping**:
A stop was requested and the Run's process tree has not yet fully ended.

**Exited**:
The Run ended on its own, successfully (exit code 0), without anyone asking it to stop.

**Failed**:
The Run ended on its own unsuccessfully (non-zero exit or signal), or could not be launched at all.

**Origin**:
Who or what requested a state-changing action — e.g. the main window, the menu bar, and later the CLI or an agent.
_Avoid_: Source, actor, caller
