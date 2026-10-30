# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Developers who work on several local projects at once: a web app, an API, a database in Compose, a worker. Today they keep a row of terminal tabs alive only to run them, and forget how an older project starts. More and more of them also let a coding agent (Claude Code, Cursor, Codex) start dev servers for them. The landing speaks to the first group first; the agent story is a strong second act.

## Product Purpose

localhost remembers how each local project runs and gives one place to start, stop and watch it. A Project is configured once; from then on the developer opens localhost, picks the Project and starts what they need. Success on the website: a visitor understands that in seconds and downloads or installs it.

## Positioning

A desktop app, not a terminal multiplexer, a production process manager or a container tool. Project-first (people think "My SaaS", not PIDs), local-first (no account, no backend, works offline) and command-agnostic (Node, Python, Bun, Cargo, Compose and plain shell commands are equal). Its `localhost mcp` server lets coding agents drive the same Services the developer sees in the window, through one shared core.

## Operating Context

Used on a developer's own machine, next to an editor and a terminal, all day. macOS (native SwiftUI), Linux and Windows (one Tauri app; Projects on Windows itself or in WSL 2), plus the `localhost` command. Install paths: direct download (DMG, NSIS installer, `.deb`, `.rpm`, AppImage) and on macOS a Homebrew cask, `brew install --cask marcellosso/tap/localhost`, which also puts `localhost` on the PATH. Releases are not notarized or signed by a known publisher yet, so the first launch on macOS and Windows needs one extra step.

## Capabilities and Constraints

The product's vocabulary is fixed and used everywhere (AGENTS.md, Vocabulary): Project, Service, Run, Port, Dependency, Environment Profile, Secret, Suggestion, Listener, the six Service States. Commands run in the login shell; the Port reaches a command only as `$PORT`; Start All goes in dependency order; nothing restarts on its own; configuration never lives in the Project's folder; Secrets live in the system's secret store. Free and open source under the MIT license.

## Brand Commitments

- The name is lowercase `localhost`, the wordmark ends in a colon in the accent colour.
- The product's "Graded glass" tokens (apps/desktop/src/styles/tokens.css): one accent (pink-red), green/orange/red/grey only for Service State, a stable colour per Service.
- Voice: short, plain, factual, uses the vocabulary; no hype.
- "Nothing silent": no automatic restarts, no Suggestions applied without review, nothing written into the user's Project.

## Evidence on Hand

- The interactive window simulation (site/src/components/AppWindow.astro) and the app's real views.
- The FAQ (site/src/lib/faq.ts), the agents page's problems, workflows and tool list (site/src/lib/agents.ts), the comparison table (site/src/lib/compare.ts), the changelog.
- No testimonials, customer logos, download counts or benchmarks. The GitHub star count is too small to show. Never fabricate any of these.
- No version numbers in UI mocks.

## Product Principles

- Show the product doing its job rather than describing it.
- Honest about friction: the unsigned first launch is explained, not hidden.
- Local-first and nothing silent are differentiators, not footnotes.
- Developers are all three platforms; macOS and Homebrew are the most common path, never the only one.

## Accessibility & Inclusion

WCAG AA text contrast in light and dark, full keyboard use, screen-reader labels on interactive mocks, and Reduce Motion respected (AGENTS.md, Accessibility).
