# SwiftUI on macOS, one Tauri app for Windows and Linux

The macOS app stays SwiftUI, and Windows and Linux share one Tauri app (Rust shell, React UI in the system webview). Using Tauri on macOS too would mean one UI to maintain, but it would throw away a finished native app and the native feel the product is built on (`localhost-v1.md` §3.3). Native toolkits per platform (WinUI, GTK) would mean three UIs. Electron was rejected for its size and memory use, given that localhost sits in the tray all day.

## Consequences

- Every user-facing feature is built twice. To keep both UIs thin, all logic (state, validation, fuzzy matching, detection, the Project file) lives in the core.
- Windows and Linux look consistent with each other rather than fully native. The Graded glass tokens are shared; translucency is optional.
- Performance depends on the system webview. WebKitGTK on Linux is the weakest, so the log view is virtualized and fed in batches (`docs/cross-platform.md`).
