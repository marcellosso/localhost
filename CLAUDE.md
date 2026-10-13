## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues (marcellosso/localhost) via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default canonical labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Project context

Native macOS (SwiftUI) local project runner. Before working on anything, read:

1. The milestone issue you're working on (`gh issue view <n> --comments`).
2. `docs/v1-decisions.md`: settled engineering decisions (overrides the spec where they differ).
3. `GLOSSARY.md` and `docs/adr/`.
4. `docs/cross-platform.md`: the Linux/Windows plan (Rust core, Tauri app); overrides `docs/v1-decisions.md` where they differ.
5. `localhost-v1.md` (product spec) and `localhost-v2.md` (future direction: MCP/CLI; shapes today's core API, do not implement).
