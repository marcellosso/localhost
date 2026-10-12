# Non-sandboxed, Developer ID distribution (no Mac App Store)

localhost must spawn arbitrary user commands through the user's login shell and signal whole process groups, which the App Sandbox does not allow. The app is therefore non-sandboxed and shipped as a Developer ID–signed, notarized build outside the Mac App Store. Minimum target is macOS 15; the look is MonoCode-style restrained materials, deliberately not Liquid Glass.

Update (M3): until there is an Apple Developer Program membership, releases are signed with a self-signed certificate (ad-hoc as a fallback) and are not notarized, so users confirm the first launch in System Settings. The release workflow switches to Developer ID and notarization when the credentials exist (`docs/v1-decisions.md`, Release).
