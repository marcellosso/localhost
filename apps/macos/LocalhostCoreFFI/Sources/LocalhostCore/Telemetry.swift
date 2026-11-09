import Foundation

// The app's anonymous usage data. The core decides what is sent and when,
// and keeps the user's choice; these add Swift names and a typed throw.

extension Telemetry {
    /// The state kept in `storageRoot`, shared with the Linux and Windows app's rules.
    public convenience init(storageRoot: URL = ProjectLibrary.defaultStorageRoot) {
        let telemetry = Telemetry.ffiOpen(storageRoot: storageRoot.path(percentEncoded: false))
        self.init(unsafeFromHandle: telemetry.uniffiCloneHandle())
    }

    /// The page that says what is sent and how to turn it off.
    public static var privacyURL: URL { URL(string: LocalhostCore.privacyUrl())! }

    /// Whether the user lets localhost send anonymous usage data. On by default.
    public var isEnabled: Bool { ffiIsEnabled() }

    /// Either way, this also counts as having seen the notice.
    public func setEnabled(_ enabled: Bool) throws(LocalhostError) {
        try core { try ffiSetEnabled(enabled: enabled) }
    }

    /// Whether to say, once, what is sent: only in a build and environment that would send.
    public var isNoticePending: Bool { ffiIsNoticePending() }

    public func acknowledgeNotice() { ffiAcknowledgeNotice() }

    /// `app_first_open` the first time, `app_opened` after it, when allowed.
    /// Returns at once: the event goes out on a thread of its own.
    public func recordAppOpen() { ffiRecordAppOpen() }
}
