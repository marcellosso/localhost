# Service commands are a single command line run through the user's login shell

A Service stores one command line (e.g. `pnpm dev`), not an executable plus an argument array, and runs it as `$SHELL -l -c "<command line>"` in its own process group. GUI apps don't inherit the PATH set up by nvm/asdf/mise/pyenv/Homebrew, so launching executables directly fails for exactly the tools developers use; and since the shell joins arguments anyway, a separate arguments field would only add quoting bugs. It also matches what users copy from READMEs.

Update (cross-platform): this holds on macOS and Linux. Native Windows execution needs its own decision on the shell and PATH (XP5, `docs/cross-platform.md`).
