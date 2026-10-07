# Service output is captured through pipes, not a pseudo-terminal

Runs write stdout and stderr to separate pipes rather than a PTY. A PTY would give terminal-identical colour and buffering but merges the two streams, and keeping stderr distinct is needed for clear failure display now and error-only log queries for agents later. We compensate with `FORCE_COLOR=1`, `CLICOLOR_FORCE=1` and `PYTHONUNBUFFERED=1` in every Run's environment and render ANSI colour in the log viewer.
