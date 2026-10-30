# Releasing localhost

Releases are on GitHub Releases: a DMG for macOS; for Linux, a `.deb`, an `.rpm` and an AppImage; and an installer for Windows. Pushing a `v*` tag runs the [Release workflow](.github/workflows/release.yml). It builds and signs the macOS app into `localhost-<version>.dmg`, and builds the Linux app (`apps/desktop/`) on Ubuntu 22.04 into `localhost-app_<version>_amd64.deb`, `localhost-app-<version>-1.x86_64.rpm` and `localhost-app_<version>_amd64.AppImage`, unsigned. For Windows it builds `localhostd` static for Linux (`x86_64-unknown-linux-musl`), then the same app on Windows into the per-user NSIS installer `localhost_<version>_x64-setup.exe`, with `localhostd` installed beside `localhost-app.exe` for the WSL distros. The installer is signed when a certificate exists (below). Every app bundles the `localhost` command of the same commit, which coding agents run as their MCP server: the macOS app in `Contents/Helpers/localhost`, signed like the app; the Linux packages as `/usr/bin/localhost`; the Windows installer as `localhost.exe` beside `localhost-app.exe`. Each job checks that it reports the release's version; for Linux, `scripts/check-linux-packages.sh` unpacks the `.deb`, the `.rpm` and the AppImage and runs the command in each. The Linux packages workflow does the same, unsigned, when run by hand. Then it drafts one GitHub Release with all of them, a SHA-256 for each, and notes generated from the merged PRs. Nothing is public until a maintainer publishes the draft.

The release also carries the update feeds that installed apps check: `latest.json` for the Linux and Windows app and `appcast.xml` for the macOS app, both naming this release's packages, each signed with the updater keys (see [Updates](#updates)). Apps read them from `releases/latest/download/`, so an update reaches users only once its release is published, and a draft or pre-release never.

## Versions

[SemVer](https://semver.org), `0.x` for now. The tag is the only place the version lives: `v0.3.1` builds version `0.3.1` (`CFBundleShortVersionString`), with the workflow's run number as the build number (`CFBundleVersion`). The Linux packages get the same version. `MARKETING_VERSION` in the Xcode project and `version` in `apps/desktop/src-tauri/tauri.conf.json` are only what local builds say. The bundled `localhost` command gets it through `LOCALHOST_VERSION` at build time; without it, it says the crates' version.

Tags are `vMAJOR.MINOR.PATCH`, nothing else; the workflow rejects any other `v*` tag.

## Cutting a release

1. Make sure `main` has everything and CI is green.
2. Run **Linux packages** by hand on the release commit, and wait for it to pass. Pull-request CI doesn't build the packages; this builds the `.deb`, the `.rpm` and the AppImage as the release does, unsigned, and checks the bundled `localhost` command in each (`scripts/check-linux-packages.sh`):

   ```sh
   gh workflow run packages.yml -R marcellosso/localhost --ref <branch>
   gh run watch -R marcellosso/localhost "$(gh run list -R marcellosso/localhost --workflow packages.yml --limit 1 --json databaseId -q '.[0].databaseId')"
   ```

3. Tag the commit and push the tag:

   ```sh
   git switch main && git pull
   git tag -a v0.3.0 -m "localhost 0.3.0"
   git push origin v0.3.0
   ```

4. Wait for the **Release** workflow (Actions tab). It ends with a draft release named "localhost 0.3.0".
5. Open the draft. Edit the notes if needed (the first line says how the build is signed and what the first launch needs). Download the DMG and check that it opens, installs and launches, following the README's first-launch steps. Install the `.deb` on Ubuntu and the `.rpm` on Fedora (or run the AppImage) and check that the app launches and its tray icon appears. On Windows 11 with Ubuntu in WSL 2, run the installer and check that a Project in the distro starts, opens in the browser, and stops when localhost quits.
6. Click **Publish release**. Installed apps see the update at their next check (at launch, or within six hours). Check that one does: open the previous version, use **Check for Updates…**, and install the update with a Service running, which should be listed and stopped first.

A fix is a normal change: branch → PR → merge → a patch tag (`v0.3.1`).

### When the workflow fails

Nothing is published, so fix the cause and tag again. To reuse the same version, delete the draft (if one was created) and the tag first:

```sh
gh release delete v0.3.0 --yes          # only if a draft exists
git push origin :refs/tags/v0.3.0
git tag -d v0.3.0
```

## Signing

The workflow signs the macOS app with the best identity it has secrets for. All three modes produce the same app; only the signature and the first-launch experience differ.

| Mode | Secrets | What users see |
|---|---|---|
| **Developer ID** | `DEVELOPER_ID_CERTIFICATE_P12`, `DEVELOPER_ID_CERTIFICATE_PASSWORD`, `NOTARY_KEY_P8`, `NOTARY_KEY_ID`, `NOTARY_ISSUER_ID` | Opens without warnings: signed with hardened runtime and a secure timestamp, notarized with `notarytool`, ticket stapled to the DMG. |
| **Self-signed** (today) | `SIGNING_CERTIFICATE_P12`, `SIGNING_CERTIFICATE_PASSWORD` | First launch needs **Open Anyway**. The Keychain keeps trusting updates, so no Secret prompts. |
| **Ad hoc** | none | First launch needs **Open Anyway**, and every update asks again for Keychain access to each Secret. |

Developer ID wins when all five of its secrets exist, then self-signed; otherwise ad hoc. Moving from one mode to another needs no workflow change.

Why the certificate matters: macOS lets an app read the Keychain items it saved when the reading app satisfies the saver's *designated requirement*. For a self-signed build that is "bundle id `io.github.marcellosso.localhost`, signed by this exact certificate", so every release signed with the same certificate is the same app to the Keychain. An ad hoc signature's requirement is the hash of that one build, which changes every release. A change of identity (a new self-signed certificate, or self-signed → Developer ID) makes every user's Mac ask once more for each Secret, on that one update. So create the certificate once and keep it.

### The self-signed certificate (one-time setup)

From a clone of the repository that will publish releases, with `gh` logged in:

```sh
scripts/create-signing-certificate.sh ~/localhost-signing --upload
```

This creates a 20-year code signing certificate named "localhost Release Signing" in a new folder (`localhost-signing.p12`, its password, and the certificate alone as `.cer`), prints its SHA-1 fingerprint, and stores it as the `SIGNING_CERTIFICATE_P12` (base64) and `SIGNING_CERTIFICATE_PASSWORD` repository secrets. Without `--upload` it only creates the files; to store them by hand:

```sh
base64 -i ~/localhost-signing/localhost-signing.p12 | gh secret set SIGNING_CERTIFICATE_P12
gh secret set SIGNING_CERTIFICATE_PASSWORD < ~/localhost-signing/localhost-signing.password
```

**Back up the folder** (a password manager or an encrypted disk, never the repository). GitHub secrets can't be read back, so the backup is the only copy: losing it means a new certificate, and Keychain prompts for every user. When the repository moves (#40), store the same `.p12` as the new repository's secrets.

To check which certificate a build was signed with, compare `codesign -dvv localhost.app` (`Authority=localhost Release Signing`) and the leaf hash in `codesign -d -r- localhost.app` with the fingerprint above.

### Developer ID and notarization (once in the Apple Developer Program)

1. In the Apple Developer account, create a **Developer ID Application** certificate. Export it with its private key from Keychain Access as a `.p12` with a password.
2. In App Store Connect → Users and Access → Integrations → **App Store Connect API**, create a team key with the **Developer** role. Download the `.p8` (only possible once) and note its Key ID and the Issuer ID.
3. Store the secrets:

   ```sh
   base64 -i DeveloperID.p12 | gh secret set DEVELOPER_ID_CERTIFICATE_P12
   gh secret set DEVELOPER_ID_CERTIFICATE_PASSWORD
   base64 -i AuthKey_XXXXXXXXXX.p8 | gh secret set NOTARY_KEY_P8
   gh secret set NOTARY_KEY_ID --body XXXXXXXXXX
   gh secret set NOTARY_ISSUER_ID --body xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
   ```

The next tag is signed with Developer ID and notarized. Users get one Keychain prompt per Secret on that update (the identity changed), then none. Update the README's install section: the first-launch steps are no longer needed. Keep the self-signed secrets until then; afterwards they are unused and can be deleted.

### Windows (Authenticode)

The Windows installer, and the app and the `localhost` command inside it, are signed when the `WINDOWS_CERTIFICATE_PFX` (the `.pfx` in base64) and `WINDOWS_CERTIFICATE_PASSWORD` secrets exist. The workflow imports the certificate into the runner's store; it signs the `localhost` command with `signtool` before Tauri packs it, and Tauri signs the rest, SHA-256 and a DigiCert timestamp both. The release notes then say "Signed." Without the secrets, the installer is unsigned: SmartScreen asks first, and the README's [Windows](README.md#windows-smartscreen) section has the steps.

```sh
base64 -i localhost-windows.pfx | gh secret set WINDOWS_CERTIFICATE_PFX
gh secret set WINDOWS_CERTIFICATE_PASSWORD
```

Use a code signing certificate from a certificate authority: SmartScreen gives a self-signed one no trust, so it does nothing for users. An OV certificate still builds reputation with each download; EV certificates usually live on hardware tokens, which this workflow can't use.

## Updates

Each release is signed for the updaters with two keypairs, separate from code signing: one for the Linux and Windows app (Tauri's updater, minisign) and one for the macOS app (Sparkle, EdDSA). Every installed app trusts only the public keys it was built with, so these are created **once and kept**: a lost private key, or a new pair, means every user must download the next version by hand.

The workflow fails at its first step, before building anything, unless all five exist:

| | Name | What |
|---|---|---|
| secret | `TAURI_SIGNING_PRIVATE_KEY` | the Tauri private key (the key file's content) |
| secret | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password |
| secret | `SPARKLE_PRIVATE_KEY` | the Sparkle private key (base64) |
| variable | `TAURI_UPDATER_PUBLIC_KEY` | the Tauri public key, built into the Linux and Windows app |
| variable | `SPARKLE_PUBLIC_KEY` | the Sparkle public key, built into the macOS app as `SUPublicEDKey` |

Failing is deliberate: a release built without them would install fine but could never update itself, stranding its users on that version. The workflow also checks that each private key is the pair of its public key before anything is published.

### Creating the keys (one-time setup)

From a clone of the repository that publishes releases, on a Mac with `gh` logged in, `pnpm install` done in `apps/desktop`, and OpenSSL 3 (`brew install openssl@3`):

```sh
scripts/create-updater-keys.sh ~/localhost-updater-keys --upload
```

It creates both keypairs in a new folder, prints the public keys, and stores the three secrets and two variables above in `marcellosso/localhost` (set `LOCALHOST_REPO=owner/name` for another repository). If the keys already exist, store them again without making new ones: `scripts/create-updater-keys.sh ~/localhost-updater-keys --upload-only`. The Sparkle key is made with OpenSSL, so nothing is added to your keychain. Without `--upload` it only creates the files; to store them by hand:

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/localhost-updater-keys/tauri-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < ~/localhost-updater-keys/tauri-updater.password
gh secret set SPARKLE_PRIVATE_KEY < ~/localhost-updater-keys/sparkle-private-key
gh variable set TAURI_UPDATER_PUBLIC_KEY < ~/localhost-updater-keys/tauri-updater.key.pub
gh variable set SPARKLE_PUBLIC_KEY < ~/localhost-updater-keys/sparkle-public-key
```

**Back up the folder** (a password manager or an encrypted disk, never the repository). When the repository moves (#40), store the same keys as the new repository's secrets and variables; the feeds' addresses follow the repository the release is built in.

### How it fits together

- **Linux and Windows**: `tauri build` gets `createUpdaterArtifacts`, the public key and this repository's `latest.json` address through `--config`, and signs the AppImage and the installer (`.sig` beside each); the workflow signs the `.deb` and `.rpm` if the bundler didn't. `scripts/updater-manifest.sh` writes `latest.json` from those signatures, checking each was made by the public key's pair. The `.sig` files go into `latest.json`, not the release. Signatures are bound to the version (`requireSignedVersion`), so a tampered `latest.json` can't pair a new version number with an older package.
- **macOS**: `scripts/build-release.sh` puts `SPARKLE_PUBLIC_KEY` (and the appcast's address, `FEED_URL`) into the app's Info.plist before signing, and signs Sparkle's helpers inside the app. After notarization, the workflow downloads Sparkle's tools (pinned by version and SHA-256), checks the private key against the public key (`scripts/sparkle-public-key.sh`), signs the DMG, writes `appcast.xml` (`scripts/appcast.sh`) and signs the appcast too: the app requires a signed feed. Sparkle compares the build number (the workflow's run number) to decide what is newer, and shows the version.
- `scripts/test-updater-scripts.sh` checks these scripts with throwaway keys; CI runs it.

Local builds have no public key, so they never check for updates.

## Building a release locally

`scripts/build-release.sh` is what the workflow runs:

```sh
scripts/build-release.sh 0.3.0                                # ad hoc
SIGNING_IDENTITY="localhost Release Signing" scripts/build-release.sh 0.3.0
```

It builds the Rust core for both architectures (`scripts/build-core.sh release`; Rust with both macOS targets, see CONTRIBUTING.md), then a universal (Apple silicon and Intel) Release app unsigned, checks it has both slices, signs it with hardened runtime and the app's entitlements, and writes `build/release/localhost-0.3.0.dmg`. The DMG is laid out by [dmgbuild](https://github.com/dmgbuild/dmgbuild), which the script installs into `build/dmgbuild-venv` the first time (needs `python3`): its window, background and icon positions are in `apps/macos/DMG/`. The Windows installer's sidebar (Welcome and Finish pages) and header images are in `apps/desktop/src-tauri/installer/`. The artwork is drawn as SVGs beside the images the builds use; after editing one, export it again: `background.png` at 660×400 and `background@2x.png` at 1320×800 (dmgbuild combines them into a Retina TIFF), and the Windows images as 24-bit BMPs, `sidebar.bmp` at 164×314 and `header.bmp` at 150×57 (NSIS takes nothing else). `SIGNING_IDENTITY` must be in a keychain on your search list (import the `.p12` into your login keychain, or a separate keychain added with `security list-keychains -s`). Notarization only runs in the workflow.
