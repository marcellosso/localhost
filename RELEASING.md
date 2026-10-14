# Releasing localhost

Releases are on GitHub Releases: a DMG for macOS and, for Linux, a `.deb`, an `.rpm` and an AppImage. Pushing a `v*` tag runs the [Release workflow](.github/workflows/release.yml). It builds and signs the macOS app into `localhost-<version>.dmg`, and builds the Linux app (`apps/desktop/`) on Ubuntu 22.04 into `localhost-app_<version>_amd64.deb`, `localhost-app-<version>-1.x86_64.rpm` and `localhost-app_<version>_amd64.AppImage`, unsigned. Then it drafts one GitHub Release with all of them, a SHA-256 for each, and notes generated from the merged PRs. Nothing is public until a maintainer publishes the draft.

There is no in-app update check: users download new versions from Releases.

## Versions

[SemVer](https://semver.org), `0.x` for now. The tag is the only place the version lives: `v0.3.1` builds version `0.3.1` (`CFBundleShortVersionString`), with the workflow's run number as the build number (`CFBundleVersion`). The Linux packages get the same version. `MARKETING_VERSION` in the Xcode project and `version` in `apps/desktop/src-tauri/tauri.conf.json` are only what local builds say.

Tags are `vMAJOR.MINOR.PATCH`, nothing else; the workflow rejects any other `v*` tag.

## Cutting a release

1. Make sure `main` has everything and CI is green.
2. Tag the commit and push the tag:

   ```sh
   git switch main && git pull
   git tag -a v0.3.0 -m "localhost 0.3.0"
   git push origin v0.3.0
   ```

3. Wait for the **Release** workflow (Actions tab). It ends with a draft release named "localhost 0.3.0".
4. Open the draft. Edit the notes if needed (the first line says how the build is signed and what the first launch needs). Download the DMG and check that it opens, installs and launches, following the README's first-launch steps. Install the `.deb` on Ubuntu and the `.rpm` on Fedora (or run the AppImage) and check that the app launches and its tray icon appears.
5. Click **Publish release**.

A fix is a normal change: branch → PR → merge → a patch tag (`v0.3.1`).

### When the workflow fails

Nothing is published, so fix the cause and tag again. To reuse the same version, delete the draft (if one was created) and the tag first:

```sh
gh release delete v0.3.0 --yes          # only if a draft exists
git push origin :refs/tags/v0.3.0
git tag -d v0.3.0
```

## Signing

The workflow signs with the best identity it has secrets for. All three modes produce the same app; only the signature and the first-launch experience differ.

| Mode | Secrets | What users see |
|---|---|---|
| **Developer ID** | `DEVELOPER_ID_CERTIFICATE_P12`, `DEVELOPER_ID_CERTIFICATE_PASSWORD`, `NOTARY_KEY_P8`, `NOTARY_KEY_ID`, `NOTARY_ISSUER_ID` | Opens without warnings: signed with hardened runtime and a secure timestamp, notarized with `notarytool`, ticket stapled to the DMG. |
| **Self-signed** (today) | `SIGNING_CERTIFICATE_P12`, `SIGNING_CERTIFICATE_PASSWORD` | First launch needs **Open Anyway**. The Keychain keeps trusting updates, so no Secret prompts. |
| **Ad hoc** | none | First launch needs **Open Anyway**, and every update asks again for Keychain access to each Secret. |

Developer ID wins when all five of its secrets exist, then self-signed; otherwise ad hoc. Moving from one mode to another needs no workflow change.

Why the certificate matters: macOS lets an app read the Keychain items it saved when the reading app satisfies the saver's *designated requirement*. For a self-signed build that is "bundle id `com.sixpetals.localhost`, signed by this exact certificate", so every release signed with the same certificate is the same app to the Keychain. An ad hoc signature's requirement is the hash of that one build, which changes every release. A change of identity (a new self-signed certificate, or self-signed → Developer ID) makes every user's Mac ask once more for each Secret, on that one update. So create the certificate once and keep it.

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

## Building a release locally

`scripts/build-release.sh` is what the workflow runs:

```sh
scripts/build-release.sh 0.3.0                                # ad hoc
SIGNING_IDENTITY="localhost Release Signing" scripts/build-release.sh 0.3.0
```

It builds the Rust core for both architectures (`scripts/build-core.sh release`; Rust with both macOS targets, see CONTRIBUTING.md), then a universal (Apple silicon and Intel) Release app unsigned, checks it has both slices, signs it with hardened runtime and the app's entitlements, and writes `build/release/localhost-0.3.0.dmg`. `SIGNING_IDENTITY` must be in a keychain on your search list (import the `.p12` into your login keychain, or a separate keychain added with `security list-keychains -s`). Notarization only runs in the workflow.
