// LocalhostCore: the UI-free core of localhost (ADR-0001).
// Never import SwiftUI or AppKit in this module.

/// Version of the core, surfaced by clients (and later the CLI/MCP).
public enum LocalhostCore {
    public static let version = "0.1.0"
}
