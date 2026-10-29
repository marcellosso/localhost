localhost on macOS is a native SwiftUI app with a menu bar companion. It needs macOS 15 or later, on Apple silicon or Intel. For the first launch of a build that isn't notarized, see [Install](/docs/install#first-launch).

## Keychain prompts for Secrets

Secret values live in your login Keychain. macOS lets the app that saved them read them without asking, and recognises “the same app” by its code signature:

- **Release builds** are signed with the same certificate every time, so updating keeps access: no prompts.
- **Builds signed ad hoc** (including ones you build yourself) look like a new app after every update. The first time each Secret is used, macOS asks “localhost wants to use your confidential information stored in “io.github.marcellosso.localhost.secret” in your keychain”. Enter your login password and click **Always Allow**.
- **When the signing identity changes** (for example once releases are notarized), macOS asks once for each Secret on that one update.
- **`localhost mcp`** is another program than the app, so macOS asks it separately the first time it reads each Secret. It reads only the Secrets of the Services an agent starts, and of their Dependencies, as it starts them.

**Deny** isn't remembered: Services that use that Secret fail to start, and the next start asks again.
