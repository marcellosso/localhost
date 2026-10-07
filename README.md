# localhost

A native macOS app that remembers how each local development project runs and lets you start, stop and inspect its Services from one place. See `localhost-v1.md` for the product spec and `docs/v1-decisions.md` for engineering decisions.

## Building and running

Requirements: macOS 15+, Xcode 26 (Swift 6.2).

```
localhost.xcodeproj           # the macOS app (SwiftUI); sources in App/
Packages/LocalhostCore        # UI-free core: state, Runs, logs, persistence
```

### App

Open `localhost.xcodeproj` in Xcode, select the `localhost` scheme and press ⌘R. Or from the command line:

```sh
xcodebuild -project localhost.xcodeproj -scheme localhost -configuration Debug -derivedDataPath build build
open build/Build/Products/Debug/localhost.app
```

To try a build without touching your real data (`~/Library/Application Support/localhost`), point it at another folder:

```sh
open -n --env LOCALHOST_STORAGE_ROOT=/tmp/localhost-dev build/Build/Products/Debug/localhost.app
```

The app is not sandboxed (ADR-0002). Code signing uses automatic signing; to build without a signing identity, add `CODE_SIGNING_ALLOWED=NO`.

### Core package

`LocalhostCore` builds and tests on its own, without Xcode's project:

```sh
cd Packages/LocalhostCore
swift build
swift test
```

The core must never import SwiftUI or AppKit (ADR-0001).
