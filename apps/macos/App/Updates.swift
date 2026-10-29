import AppKit
import Observation
import Sparkle

/// Updates through Sparkle. The release workflow publishes `appcast.xml`,
/// signed, with every release, and the DMG signed with the updater key;
/// Sparkle checks it at launch and every few hours (unless turned off in
/// Settings), and on Check for Updates…. This is Sparkle's user driver: a
/// newer version shows as a notice in the sidebar, as in the Linux and
/// Windows app, instead of Sparkle's own windows. Update Now downloads it
/// in the background and Sparkle verifies it against the public key in
/// Info.plist; Restart to Update installs it.
///
/// Installing quits the app, so, like quitting, it stops every Service
/// first, after saying which ones are running. Then it retires the shared
/// core, which runs from the app's own executable, so the new version
/// starts a core of its own instead of reaching the old one.
///
/// A build without the public key (any local build) doesn't check at all.
@Observable
final class UpdateController: NSObject {
    enum State: Equatable {
        /// No newer version known, or the notice was put off with Later.
        case none
        /// `problem` says why a download failed.
        case available(version: String, notesURL: URL, problem: String?)
        /// `progress` is from 0 to 1, `nil` while the size isn't known.
        case downloading(version: String, notesURL: URL, progress: Double?)
        /// Downloaded and verified: restarting installs it.
        case ready(version: String, notesURL: URL)
    }

    private(set) var state = State.none

    /// Whether Sparkle checks at launch and every few hours; Sparkle keeps it.
    var isAutomaticCheckEnabled: Bool {
        didSet { updater?.automaticallyChecksForUpdates = isAutomaticCheckEnabled }
    }

    /// Stops every Service and retires the shared core before an install.
    @ObservationIgnored weak var model: AppModel?

    @ObservationIgnored private var updater: SPUUpdater?
    /// Sparkle's answer to the update found, waiting for Update Now or Later.
    @ObservationIgnored private var foundReply: ((SPUUserUpdateChoice) -> Void)?
    /// Sparkle's answer once downloaded, waiting for Restart to Update.
    @ObservationIgnored private var readyReply: ((SPUUserUpdateChoice) -> Void)?
    /// Whether Check for Updates… asked, so finding nothing is said.
    @ObservationIgnored private var isUserInitiated = false
    /// Whether the version found downloads at once: Try Again after a failure.
    @ObservationIgnored private var downloadsWhenFound = false
    @ObservationIgnored private var expectedLength: UInt64 = 0
    @ObservationIgnored private var receivedLength: UInt64 = 0

    /// Where a release's notes are when the appcast doesn't say.
    private static let releases = URL(string: "https://github.com/marcellosso/localhost/releases")!

    /// Whether the release workflow built this app with the updater's public key.
    static var isConfigured: Bool {
        let key = Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") as? String
        return !(key ?? "").trimmingCharacters(in: .whitespaces).isEmpty
    }

    override init() {
        isAutomaticCheckEnabled = true
        super.init()
        guard Self.isConfigured else { return }
        let updater = SPUUpdater(hostBundle: .main, applicationBundle: .main, userDriver: self, delegate: nil)
        do {
            try updater.start()
        } catch {
            FileHandle.standardError.write(Data("localhost: updates are off: \(error.localizedDescription)\n".utf8))
            return
        }
        self.updater = updater
        isAutomaticCheckEnabled = updater.automaticallyChecksForUpdates
        // Sparkle's schedule counts from the last check; this one is at launch.
        if updater.automaticallyChecksForUpdates {
            updater.checkForUpdatesInBackground()
        }
    }

    // MARK: Actions

    /// Check for Updates…: checks now, and shows a version put off with Later again.
    func checkForUpdates() {
        guard let updater else {
            Self.inform("This build of localhost doesn’t update itself",
                        "Download new versions from GitHub Releases.")
            return
        }
        guard updater.canCheckForUpdates else {
            // A check or download is under way; the notice shows it.
            model?.showMainWindow?()
            return
        }
        isUserInitiated = true
        updater.checkForUpdates()
    }

    /// Try Again after a failed update: Sparkle has let go of it, so this
    /// checks again and downloads what it finds.
    func retry() {
        downloadsWhenFound = true
        checkForUpdates()
    }

    /// Update Now: downloads in the background. Nothing is installed until Restart to Update.
    func download() {
        guard let reply = foundReply, case .available(let version, let notesURL, _) = state else { return }
        foundReply = nil
        state = .downloading(version: version, notesURL: notesURL, progress: nil)
        reply(.install)
    }

    /// Later: hides the notice until the next check finds a version.
    func putOff() {
        foundReply?(.dismiss)
        foundReply = nil
        state = .none
    }

    /// Restart to Update: asks first if any Service is active, since quitting
    /// stops them, then lets Sparkle install and relaunch.
    func install() {
        guard let reply = readyReply, case .ready(let version, _) = state else { return }
        let active = model?.activeServicesByProject ?? []
        if !active.isEmpty {
            let count = active.reduce(0) { $0 + $1.services.count }
            NSApp.activate()
            let alert = NSAlert()
            alert.messageText = count == 1 ? "Stop 1 service and update?" : "Stop \(count) services and update?"
            let list = active.map { "\($0.project): \($0.services.formatted(.list(type: .and)))" }.joined(separator: "\n")
            alert.informativeText = "localhost restarts to install version \(version). These services are stopped first:\n\n\(list)"
            alert.addButton(withTitle: "Stop and Update")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return }
        }
        readyReply = nil
        Task {
            await model?.prepareForUpdate()
            reply(.install)
        }
    }

    func openReleaseNotes() {
        switch state {
        case .available(_, let url, _), .downloading(_, let url, _), .ready(_, let url): NSWorkspace.shared.open(url)
        case .none: break
        }
    }

    private static func inform(_ title: String, _ message: String) {
        NSApp.activate()
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.runModal()
    }

    private static func notesURL(of item: SUAppcastItem) -> URL {
        item.fullReleaseNotesURL ?? releases.appending(path: "tag/v\(item.displayVersionString)")
    }
}

// MARK: Sparkle's user driver

extension UpdateController: SPUUserDriver {
    func show(_ request: SPUUpdatePermissionRequest, reply: @escaping (SUUpdatePermissionResponse) -> Void) {
        // Info.plist turns automatic checks on, so Sparkle doesn't ask; Settings turns them off.
        reply(SUUpdatePermissionResponse(automaticUpdateChecks: true, sendSystemProfile: false))
    }

    func showUserInitiatedUpdateCheck(cancellation: @escaping () -> Void) {}

    func showUpdateFound(with appcastItem: SUAppcastItem, state: SPUUserUpdateState, reply: @escaping (SPUUserUpdateChoice) -> Void) {
        isUserInitiated = false
        defer { downloadsWhenFound = false }
        let version = appcastItem.displayVersionString
        let notesURL = Self.notesURL(of: appcastItem)
        switch state.stage {
        case .notDownloaded where downloadsWhenFound:
            self.state = .downloading(version: version, notesURL: notesURL, progress: nil)
            reply(.install)
        case .notDownloaded:
            foundReply = reply
            self.state = .available(version: version, notesURL: notesURL, problem: nil)
        // Downloaded in an earlier session: ready at once.
        case .downloaded, .installing:
            self.state = .downloading(version: version, notesURL: notesURL, progress: nil)
            reply(.install)
        @unknown default:
            reply(.dismiss)
        }
        if state.userInitiated { model?.showMainWindow?() }
    }

    func showUpdateReleaseNotes(with downloadData: SPUDownloadData) {}

    func showUpdateReleaseNotesFailedToDownloadWithError(_ error: any Error) {}

    func showUpdateNotFoundWithError(_ error: any Error, acknowledgement: @escaping () -> Void) {
        downloadsWhenFound = false
        if case .available(_, _, .some) = state { state = .none }
        if isUserInitiated {
            let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
            Self.inform("localhost is up to date", "Version \(version) is the latest.")
        }
        isUserInitiated = false
        acknowledgement()
    }

    func showUpdaterError(_ error: any Error, acknowledgement: @escaping () -> Void) {
        switch state {
        case .downloading(let version, let notesURL, _), .ready(let version, let notesURL):
            // Sparkle forgets the update; the next check offers it again.
            state = .available(version: version, notesURL: notesURL, problem: "The update failed: \(error.localizedDescription)")
        default:
            downloadsWhenFound = false
            if isUserInitiated {
                Self.inform("localhost couldn’t check for updates", "\(error.localizedDescription) Check your connection and try again.")
            }
        }
        isUserInitiated = false
        acknowledgement()
    }

    func showDownloadInitiated(cancellation: @escaping () -> Void) {
        expectedLength = 0
        receivedLength = 0
    }

    func showDownloadDidReceiveExpectedContentLength(_ expectedContentLength: UInt64) {
        expectedLength = expectedContentLength
    }

    func showDownloadDidReceiveData(ofLength length: UInt64) {
        receivedLength += length
        guard case .downloading(let version, let notesURL, let previous) = state, expectedLength > 0 else { return }
        let progress = min(Double(receivedLength) / Double(expectedLength), 1)
        // A step a percent at least, so the view isn't redrawn per packet.
        if previous.map({ progress - $0 >= 0.01 }) ?? true {
            state = .downloading(version: version, notesURL: notesURL, progress: progress)
        }
    }

    func showDownloadDidStartExtractingUpdate() {}

    func showExtractionReceivedProgress(_ progress: Double) {}

    func showReady(toInstallAndRelaunch reply: @escaping (SPUUserUpdateChoice) -> Void) {
        switch state {
        case .downloading(let version, let notesURL, _), .available(let version, let notesURL, _):
            readyReply = reply
            state = .ready(version: version, notesURL: notesURL)
        case .ready, .none:
            readyReply = reply
        }
    }

    func showInstallingUpdate(withApplicationTerminated applicationTerminated: Bool, retryTerminatingApplication: @escaping () -> Void) {}

    func showUpdateInstalledAndRelaunched(_ relaunched: Bool, acknowledgement: @escaping () -> Void) {
        acknowledgement()
    }

    func showUpdateInFocus() {
        model?.showMainWindow?()
    }

    func dismissUpdateInstallation() {
        foundReply = nil
        readyReply = nil
        // A failed download keeps its notice and says why.
        if case .available(_, _, .some) = state { return }
        state = .none
    }
}
