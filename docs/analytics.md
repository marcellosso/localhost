# Website analytics

The website in `site/` (getlocalhost.app) sends anonymous usage events to PostHog, project `localhost` (US Cloud). The localhost apps and the `localhost` command send nothing.

## Setup

- `site/src/lib/analytics.ts` starts `posthog-js` on production builds only; `astro dev` sends nothing.
- Events go to `/ingest`, which `site/vercel.json` rewrites to `us.i.posthog.com` (and `/ingest/static`, `/ingest/array` to `us-assets.i.posthog.com`), so blockers that list PostHog's hosts let them through.
- `PUBLIC_POSTHOG_KEY` and `PUBLIC_POSTHOG_HOST` override the token and host (`site/.env.example`). The token is public: it can only send events.
- Captured automatically: `$pageview`, `$pageleave`, autocapture (clicks, form changes). Session recording is off. Every event carries `product: "localhost"`.

To check a build locally, where `/ingest` doesn't exist:

```sh
cd site && PUBLIC_POSTHOG_HOST=https://us.i.posthog.com pnpm build && pnpm preview
```

## Events

| Event | When | Properties |
|---|---|---|
| `download_clicked` | A Download button or link | `location` (`header`, `hero`, `closing`, `footer`, `install_dialog`); `platform` (`macos`, `windows`, `linux`) on the Download buttons; `package` (`mac`, `windows`, `deb`, `rpm`, `appimage`) in the install dialog |
| `install_command_copied` | A Copy button in the install dialog | `command` (`curl`, `xattr`, `apt`, `dnf`, `appimage`) |
| `install_opened` | A platform's link in the landing page's install section, opening the install dialog | `platform` (`macos`, `windows`, `linux`) |
| `platform_docs_clicked` | A platform's name in the hero, linking to its docs page | `platform` (`macos`, `windows`, `linux`); `location` (`hero`) |
| `github_clicked` | A link to the repository | `location` (`header`, `hero`, `footer`) |

An event is declared in the markup: `data-track="<event>"` on the element, and a `data-track-<property>="<value>"` attribute per property. Keep names in `snake_case`, as `<object>_<past-tense verb>`, and add new ones to this table.
