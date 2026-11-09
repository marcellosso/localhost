# Analytics

localhost measures two things: what people do on the website before downloading, and whether the apps are opened afterwards. Both send anonymous events to PostHog (US Cloud), each to its own project, and never link one to the other. The `localhost` command, `localhost mcp` and `localhostd` send nothing. What users are told is on [getlocalhost.app/privacy](https://www.getlocalhost.app/privacy) (`site/src/pages/privacy.astro`); keep it in step with this file.

The tokens built into both are PostHog project tokens: public, and only able to send events.

## The apps

The macOS app and the Linux and Windows app send two events, through `Telemetry` in the core (`core/localhost-core/src/telemetry.rs`). The project is separate from the site's, so app events never mix with page views.

| Event | When |
|---|---|
| `app_first_open` | The first open of this install whose event was accepted; tried again at the next open until it is |
| `app_opened` | Every open after that |

Every event has exactly these properties, and a test fails if one is added:

| Property | Value |
|---|---|
| `distinct_id` | A random UUID made the first time an event is sent, kept in `telemetry.json` in the storage root |
| `app_version` | The core's `VERSION`, which release builds set from `LOCALHOST_VERSION` |
| `os` | `macos`, `windows`, `linux` |
| `arch` | `aarch64`, `x86_64`, … |
| `install_channel` | `brew`, `dmg`, `exe`, `deb`, `rpm`, `appimage`, `source`, `unknown`: a guess from where the app runs, never the path itself |
| `product` | `localhost-app` |
| `$process_person_profile` | `false`: no person profile |
| `$geoip_disable` | `true`: no location from the IP address |

Nothing else: no paths, Project or Service names, commands, logs, host names or user names.

### When nothing is sent

Any one of these keeps the app silent; the first-open notice doesn't show either, since there is nothing to say.

- The build was made with `LOCALHOST_POSTHOG_KEY` set but empty (see below), or the core was built without its `telemetry` feature. Only the apps turn it on (`core/localhost-ffi`, `apps/desktop/src-tauri`), so the `localhost` command and `localhostd` don't even contain an HTTP client.
- It is a debug build, which includes every test run.
- `CI` is set.
- `DO_NOT_TRACK` is set to anything but `0`, `false`, `no`, `off` or nothing.
- `LOCALHOST_TELEMETRY` is `0`, `false`, `no` or `off`.
- The user turned off Settings › General › **Share anonymous usage data**. The choice is kept in `telemetry.json`, so both apps read it the same way; a `telemetry.json` that can't be read counts as off and is never overwritten.

The variables are read from the app's own environment: on macOS that of `open` or launchd (`launchctl setenv DO_NOT_TRACK 1`), on Linux and Windows the session's.

### The notice

The first time a build that would send opens, the foot of the sidebar says what is sent, that it is on, and how to turn it off (Settings › General, `DO_NOT_TRACK=1`), linking to the privacy page. OK or Settings… dismisses it for good, as does changing the setting.

### Sending

`record_app_open` returns at once: the request is a plain `POST <host>/i/v0/e/` with a JSON body, made with `ureq` on a thread of its own, with a 5 s timeout. Failures are dropped and never shown.

### The key

The app project's token is built in (`DEFAULT_KEY` in `telemetry.rs`); like the site's, it is public and can only send events. Debug builds, tests and CI never use it. Two build-time variables, read like `LOCALHOST_VERSION`, change it:

- `LOCALHOST_POSTHOG_KEY`: another project's token instead; set but empty, the build never sends.
- `LOCALHOST_POSTHOG_HOST`: where events go, `https://us.i.posthog.com` by default; a first-party proxy can replace it without a code change. The release workflow passes the repository's Actions variable of the same name, if there is one.

In the app project's settings, keep **Discard client IP data** on.

To check a build by hand, point it at a throwaway project and open the release build from a terminal without `CI` set:

```sh
LOCALHOST_POSTHOG_KEY=phc_throwaway scripts/build-release.sh 0.0.0-test 1   # macOS
cd apps/desktop && LOCALHOST_POSTHOG_KEY=phc_throwaway pnpm tauri build       # Linux, Windows
```

The project's Activity shows `app_first_open`, then `app_opened` at the next open. The same build opened with `DO_NOT_TRACK=1`, or built with `LOCALHOST_POSTHOG_KEY=`, sends nothing.

## The website

The website in `site/` (getlocalhost.app) sends to the PostHog project `localhost`.

### Setup

- `site/src/lib/analytics.ts` starts `posthog-js` on production builds only; `astro dev` sends nothing.
- Events go to `/ingest`, which `site/vercel.json` rewrites to `us.i.posthog.com` (and `/ingest/static`, `/ingest/array` to `us-assets.i.posthog.com`), so blockers that list PostHog's hosts let them through.
- `PUBLIC_POSTHOG_KEY` and `PUBLIC_POSTHOG_HOST` override the token and host (`site/.env.example`).
- Captured: `$pageview`, `$pageleave`, and the events below. Autocapture and session recording are off. Every event carries `product: "localhost"`.

To check a build locally, where `/ingest` doesn't exist:

```sh
cd site && PUBLIC_POSTHOG_HOST=https://us.i.posthog.com pnpm build && pnpm preview
```

### Events

| Event | When | Properties |
|---|---|---|
| `download_clicked` | A Download button or link | `location` (`header`, `hero`, `closing`, `footer`, `install_dialog`); `platform` (`macos`, `windows`, `linux`) on the Download buttons; `package` (`mac`, `windows`, `deb`, `rpm`, `appimage`) in the install dialog |
| `install_command_copied` | A Copy button in the install dialog, the landing page or /agents | `command` (`curl`, `xattr`, `apt`, `dnf`, `appimage`, `brew`, `npx`); `location` where the page sets it |
| `install_opened` | A platform's link in the landing page's install section, opening the install dialog | `platform` (`macos`, `windows`, `linux`) |
| `platform_docs_clicked` | A platform's name in the hero, linking to its docs page | `platform` (`macos`, `windows`, `linux`); `location` (`hero`) |
| `github_clicked` | A link to the repository | `location` (`header`, `hero`, `footer`) |

An event is declared in the markup: `data-track="<event>"` on the element, and a `data-track-<property>="<value>"` attribute per property. Keep names in `snake_case`, as `<object>_<past-tense verb>`, and add new ones to this table.
