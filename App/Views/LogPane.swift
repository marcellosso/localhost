import AppKit
import LocalhostCore
import Synchronization
import SwiftUI

/// The selected Service's output: its latest Run, live.
struct LogPane: View {
    @Environment(AppModel.self) private var model
    let service: Service
    /// The Failed Run whose banner was put away with "Show in Log".
    @State private var dismissedBannerRunID: UUID?
    @State private var revealCount = 0

    var body: some View {
        let status = model.status(of: service)
        VStack(spacing: 0) {
            LogHeader {
                RoundedRectangle(cornerRadius: 2)
                    .fill(Color(nsColor: model.color(of: service)))
                    .frame(width: 8, height: 8)
                Text(service.name).fontWeight(.semibold)
                    .lineLimit(1)
                    .layoutPriority(1)
                Text(status.summary)
                    .accessibilityLabel(status.spokenSummary)
                    .foregroundStyle(status.state == .failed ? AnyShapeStyle(Color(nsColor: Theme.failed))
                        : status.hasPortWarning ? AnyShapeStyle(Color(nsColor: Theme.transitioning)) : AnyShapeStyle(.secondary))
                    .lineLimit(1)
                    .layoutPriority(1)
                // Without room, the run time goes first, then the URL.
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) {
                        runTime(status)
                        url(status)
                    }
                    url(status)
                    Color.clear.frame(width: 0, height: 0)
                }
            }

            if let logs = model.core?.logs {
                if status.state == .failed, let run = status.run, run.id != dismissedBannerRunID {
                    FailedBanner(service: service, run: run, logs: logs) {
                        dismissedBannerRunID = run.id
                        revealCount += 1
                    }
                }
                LogView(source: .run(status.run?.id), logs: logs, filter: model.logFilter,
                        clearCount: model.logClearCount, revealCount: revealCount)
                    .overlay {
                        if status.run == nil {
                            VStack(spacing: 10) {
                                Text("No output yet.")
                                    .foregroundStyle(.secondary)
                                if status.state == .stopped {
                                    Button("Start \(service.name)", systemImage: "play.fill") { model.start(service.id) }
                                        .buttonStyle(AccentButtonStyle())
                                        .help("Start (⌘R)")
                                }
                            }
                        }
                    }
            }
        }
        .background(Color(nsColor: Theme.logBackground), ignoresSafeAreaEdges: [])
    }

    @ViewBuilder
    private func runTime(_ status: ServiceStatus) -> some View {
        if let run = status.run { RunTime(run: run) }
    }

    @ViewBuilder
    private func url(_ status: ServiceStatus) -> some View {
        if status.state.isActive, let url = model.urlToOpen(service) {
            Button { model.openURL(of: service.id) } label: {
                HStack(spacing: 2) {
                    Text(url.absoluteString.replacingOccurrences(of: "http://", with: ""))
                        .font(.callout.monospaced())
                        .lineLimit(1)
                    Image(systemName: "arrow.up.right").font(.system(size: 8.5, weight: .bold))
                }
                .foregroundStyle(Theme.accentText)
            }
            .buttonStyle(.plain)
            .fixedSize()
            .help("Open \(url.absoluteString) (⇧⌘O)")
            .accessibilityLabel("Open \(url.absoluteString) in Browser")
        }
    }
}

/// Every Service of a Project in one stream, each line prefixed with its
/// Service's Key in the Service's colour.
struct CombinedLogPane: View {
    @Environment(AppModel.self) private var model
    let project: Project

    var body: some View {
        let running = project.services.filter { model.status(of: $0).state.isActive }.count
        VStack(spacing: 0) {
            LogHeader {
                ServiceDots(services: model.serviceDots(of: project))
                Text("All services").fontWeight(.semibold)
                Text("\(running) of \(project.services.count) running")
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if let logs = model.core?.logs {
                LogView(source: .services(Self.prefixes(of: project)), logs: logs, filter: model.logFilter,
                        clearCount: model.logClearCount, revealCount: 0)
            }
        }
        .background(Color(nsColor: Theme.logBackground), ignoresSafeAreaEdges: [])
    }

    /// Each Service's Key, padded to the longest, in a colour stable for its
    /// position in the Project.
    static func prefixes(of project: Project) -> [UUID: LogPrefix] {
        let width = project.services.map(\.key.rawValue.count).max() ?? 0
        var prefixes: [UUID: LogPrefix] = [:]
        for (index, service) in project.services.enumerated() {
            let key = service.key.rawValue
            prefixes[service.id] = LogPrefix(
                text: key.padding(toLength: width, withPad: " ", startingAt: 0),
                color: Theme.serviceColor(at: index)
            )
        }
        return prefixes
    }
}

/// The bar above a log: what it shows, then the filter and a `⋯` menu.
private struct LogHeader<Content: View>: View {
    @Environment(AppModel.self) private var model
    @FocusState private var isFilterFocused: Bool
    @ViewBuilder let content: Content

    var body: some View {
        @Bindable var model = model
        HStack(spacing: 8) {
            content
            Spacer(minLength: 8)
            HStack(spacing: 5) {
                Image(systemName: "line.3.horizontal.decrease")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                TextField("Filter", text: $model.logFilter)
                    .textFieldStyle(.plain)
                    .focused($isFilterFocused)
                    .onExitCommand { model.logFilter = "" }
                    // ↩ keeps the filter and moves into the log to read it.
                    .onSubmit { model.focusColumn(.detail) }
                if model.logFilter.isEmpty, !isFilterFocused {
                    KeyHint("⌘F")
                } else if !model.logFilter.isEmpty {
                    Button("Clear Filter", systemImage: "xmark.circle.fill") { model.logFilter = "" }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .foregroundStyle(.secondary)
                }
            }
            .font(.callout)
            .padding(.horizontal, 7)
            .frame(width: 170, height: 24)
            .background(Color.primary.opacity(0.06), in: .rect(cornerRadius: 6))
            .help("Show only lines containing this text (⌘F)")
            MoreMenu(help: "More log actions") {
                Button("Copy Log") { NSApp.sendAction(#selector(LogTextView.copyLog(_:)), to: nil, from: nil) }
                Button("Clear Log") { model.clearLogs() }
                    .keyboardShortcut("l", modifiers: .control)
            }
        }
        .font(.callout)
        .padding(.leading, 16)
        .padding(.trailing, 10)
        .frame(height: 40)
        .background(Color(nsColor: Theme.logBackground), ignoresSafeAreaEdges: [])
        .overlay(alignment: .bottom) { Divider() }
        .onChange(of: model.logFindCount) { isFilterFocused = true }
    }
}

/// How long the active Run has been up, or when the last one ended, kept current.
private struct RunTime: View {
    let run: Run

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { context in
            Text(text(now: context.date))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .help("Started \(run.startedAt.formatted(date: .abbreviated, time: .standard))"
                      + (run.endedAt.map { ", ended \($0.formatted(date: .omitted, time: .standard))" } ?? ""))
        }
    }

    private func text(now: Date) -> String {
        if let endedAt = run.endedAt {
            return "· " + endedAt.formatted(date: .omitted, time: .shortened)
        }
        let seconds = max(0, now.timeIntervalSince(run.startedAt))
        if seconds < 60 { return "· just now" }
        return "· " + (Duration.seconds(seconds).formatted(.units(allowed: [.days, .hours, .minutes], width: .narrow, maximumUnitCount: 2)))
    }
}

/// Why a Service Failed: what ended it and when, the error line from its
/// output when the log below doesn't already show it all, and Start / Edit.
private struct FailedBanner: View {
    @Environment(AppModel.self) private var model
    let service: Service
    let run: Run
    let logs: LogStore
    let dismiss: () -> Void
    @State private var excerpt: [OutputLine] = []

    /// About a pane of output: with no more than this, the log below shows
    /// the error already, so the banner doesn't repeat it.
    static let shortRunLineCount = 30
    static let excerptLineCount = 4

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 9) {
                StateIndicator(state: .failed)
                Text(title)
                    .fontWeight(.semibold)
                    .foregroundStyle(Color(nsColor: Theme.failed))
                    .lineLimit(2)
                if let detail {
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 8)
                Button("Start") { model.start(service.id) }
                    .buttonStyle(AccentButtonStyle(isCompact: true))
                    .help("Start (⌘R)")
                Button("Edit Service…") { model.presentedSheet = .editService(service.id) }
                    .buttonStyle(PillButtonStyle(isCompact: true))
                Button("Hide", systemImage: "xmark") { dismiss() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .help("Hide this and show the end of the log")
            }
            if !excerpt.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(excerpt.enumerated()), id: \.offset) { _, line in
                        Text(ANSI.strip(line.text))
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                .font(.system(size: 11.5, design: .monospaced))
                .textSelection(.enabled)
                .padding(.leading, 10)
                .overlay(alignment: .leading) {
                    Rectangle().fill(Color(nsColor: Theme.failed).opacity(0.55)).frame(width: 2)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(Color(nsColor: Theme.failed).opacity(0.08), in: .rect(cornerRadius: 8))
        .overlay { RoundedRectangle(cornerRadius: 8).strokeBorder(Color(nsColor: Theme.failed).opacity(0.25)) }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 4)
        .task(id: run.id) {
            // Lines may still arrive after the Service turned Failed.
            let (_, updates) = await logs.subscribe(toRun: run.id)
            excerpt = Self.excerpt(of: await logs.recentOutput(forRun: run.id, limit: 40))
            for await _ in updates {
                excerpt = Self.excerpt(of: await logs.recentOutput(forRun: run.id, limit: 40))
            }
        }
    }

    /// e.g. "Worker exited with code 1", or the typed reason ("Command not found.").
    private var title: String {
        if let failure = run.failure { return failure.message }
        switch run.termination {
        case .exited(let code): return "\(service.name) exited with code \(code)"
        case .signaled(let signal): return "\(service.name) was ended by \(Termination.signalName(signal))"
        case nil: return "\(service.name) failed"
        }
    }

    /// How long it ran, e.g. "after 1.2s".
    private var detail: String? {
        guard let endedAt = run.endedAt else { return nil }
        let seconds = endedAt.timeIntervalSince(run.startedAt)
        if seconds < 60 { return "after \(seconds.formatted(.number.precision(.fractionLength(1))))s" }
        return "after " + Duration.seconds(seconds).formatted(.units(allowed: [.hours, .minutes, .seconds], width: .narrow, maximumUnitCount: 2))
    }

    /// The first line that reads like an error and the few after it, else the
    /// last lines; nothing for short output, which the log shows whole.
    static func excerpt(of lines: [OutputLine]) -> [OutputLine] {
        let output = lines.filter { $0.channel != .localhost }
        guard output.count > shortRunLineCount else { return [] }
        let pattern = /(?i)\b(error|exception|fatal|panic|traceback|failed)\b/
        if let start = output.firstIndex(where: { ANSI.strip($0.text).contains(pattern) }) {
            return Array(output[start...].prefix(excerptLineCount))
        }
        return Array(output.suffix(excerptLineCount))
    }
}

/// An opaque, selectable, monospaced log surface backed by `NSTextView`.
/// It follows new output while scrolled to the bottom and pauses while the
/// user has scrolled up.
struct LogView: NSViewRepresentable {
    enum Source: Equatable {
        /// One Run's output.
        case run(UUID?)
        /// The combined stream of these Services.
        case services([UUID: LogPrefix])

        /// What is subscribed to, leaving out how lines are prefixed.
        var streamID: AnyHashable {
            switch self {
            case .run(let runID): AnyHashable(runID)
            case .services(let prefixes): AnyHashable(Set(prefixes.keys))
            }
        }
    }

    let source: Source
    let logs: LogStore
    @AppStorage(Preferences.logFontSizeKey) private var fontSize = Preferences.defaultLogFontSize
    /// Only lines containing this text are shown, with matches highlighted.
    let filter: String
    /// Changes when the buffers were cleared: show the log again from scratch.
    let clearCount: Int
    /// Changes to scroll to the newest output.
    let revealCount: Int

    func makeCoordinator() -> LogViewController {
        LogViewController(logs: logs)
    }

    func makeNSView(context: Context) -> NSScrollView {
        context.coordinator.scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.setFontSize(fontSize)
        context.coordinator.show(source, clearCount: clearCount)
        context.coordinator.setFilter(filter)
        context.coordinator.reveal(revealCount)
    }

    static func dismantleNSView(_ scrollView: NSScrollView, coordinator: LogViewController) {
        coordinator.stop()
    }
}

final class LogViewController {
    /// Lines kept in the view; matches the core's per-Run buffer.
    static let maxLines = LogStore.defaultMaxLinesPerRun

    /// A line as kept by the view, with the Service it came from in the combined stream.
    struct Line {
        let line: OutputLine
        let serviceID: UUID?
    }

    let scrollView = NSScrollView()
    private let textView = LogTextView(frame: .zero)
    private let logs: LogStore
    private var source: LogView.Source?
    private var clearCount = 0
    private var filter = ""
    private var fontSize = Preferences.defaultLogFontSize
    private var revealCount = 0
    private var subscription: Task<Void, Never>?
    private var feeder: Task<Void, Never>?
    /// Every line received (up to `maxLines`), whether the filter shows it or not.
    private var lines: [Line] = []
    /// UTF-16 length of each displayed line, oldest first, for trimming.
    private var displayedLengths: [Int] = []

    init(logs: LogStore) {
        self.logs = logs
        textView.isEditable = false
        textView.isSelectable = true
        textView.isRichText = true
        textView.allowsUndo = false
        textView.drawsBackground = true
        textView.backgroundColor = Theme.logBackground
        textView.textContainerInset = NSSize(width: 6, height: 8)
        textView.minSize = .zero
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textView.setAccessibilityLabel("Log output")
        textView.didScrollFromKeyboard = { [weak self] in self?.updateFollowing() }

        scrollView.documentView = textView
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = true
        scrollView.backgroundColor = Theme.logBackground
        scrollView.borderType = .noBorder
        // Following is decided by the user's scrolling, not by where the view
        // happens to be: a banner or a resize must not stop it.
        let center = NotificationCenter.default
        observers.append(center.addObserver(forName: NSScrollView.didEndLiveScrollNotification, object: scrollView, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.updateFollowing() }
        })
        scrollView.postsFrameChangedNotifications = true
        observers.append(center.addObserver(forName: NSView.frameDidChangeNotification, object: scrollView, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, self.isFollowing else { return }
                self.textView.scrollToEndOfDocument(nil)
            }
        })
    }

    func show(_ source: LogView.Source, clearCount: Int) {
        guard source != self.source || clearCount != self.clearCount else { return }
        if clearCount == self.clearCount, let previous = self.source, source.streamID == previous.streamID {
            // Only a Key or colour changed.
            self.source = source
            render()
            return
        }
        self.source = source
        self.clearCount = clearCount
        subscribe(to: source)
    }

    /// Shows the log again in the new size.
    func setFontSize(_ size: Double) {
        guard size != fontSize else { return }
        fontSize = size
        render()
    }

    func setFilter(_ filter: String) {
        guard filter != self.filter else { return }
        self.filter = filter
        render()
    }

    func reveal(_ count: Int) {
        guard count != revealCount else { return }
        revealCount = count
        isFollowing = true
        textView.scrollToEndOfDocument(nil)
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    func stop() {
        subscription?.cancel()
        feeder?.cancel()
    }

    // MARK: Receiving

    private func subscribe(to source: LogView.Source) {
        stop()
        lines = []
        render()
        let batcher = LineBatcher<Line>(limit: Self.maxLines)

        feeder = Task.detached { [logs] in
            switch source {
            case .run(nil):
                break
            case .run(let runID?):
                let (lines, updates) = await logs.subscribe(toRun: runID)
                batcher.append(lines.map { Line(line: $0, serviceID: nil) })
                for await line in updates { batcher.append([Line(line: line, serviceID: nil)]) }
            case .services(let prefixes):
                let (lines, updates) = await logs.subscribe(toServices: Set(prefixes.keys))
                batcher.append(lines.map { Line(line: $0.line, serviceID: $0.serviceID) })
                for await line in updates { batcher.append([Line(line: line.line, serviceID: line.serviceID)]) }
            }
            batcher.finish()
        }
        // Chatty Services can print thousands of lines a second: they are
        // collected off the main actor and shown in one batch per ~50ms, or
        // less often while showing a batch takes long, so the UI stays responsive.
        subscription = Task { [weak self] in
            while !Task.isCancelled {
                let start = ContinuousClock.now
                let (lines, isFinished) = batcher.drain()
                self?.append(lines)
                if isFinished { return }
                try? await Task.sleep(for: max(.milliseconds(50), (ContinuousClock.now - start) * 3))
            }
        }
    }

    private func append(_ newLines: [Line]) {
        guard !newLines.isEmpty, let storage = textView.textStorage else { return }
        let followOutput = isFollowing
        lines += newLines
        if lines.count > Self.maxLines + 1_000 {
            lines.removeFirst(lines.count - Self.maxLines)
        }
        let text = NSMutableAttributedString()
        // A burst larger than the view keeps would be trimmed right away.
        for line in newLines.suffix(Self.maxLines) where matches(line) {
            let attributed = attributedLine(line)
            displayedLengths.append(attributed.length)
            text.append(attributed)
        }
        storage.beginEditing()
        storage.append(text)
        if displayedLengths.count > Self.maxLines + 1_000 {
            let excess = displayedLengths.count - Self.maxLines
            let length = displayedLengths.prefix(excess).reduce(0, +)
            storage.deleteCharacters(in: NSRange(location: 0, length: length))
            displayedLengths.removeFirst(excess)
        }
        storage.endEditing()
        if followOutput {
            textView.scrollToEndOfDocument(nil)
        }
    }

    /// Shows the kept lines again, e.g. after the filter changed.
    private func render() {
        guard let storage = textView.textStorage else { return }
        let text = NSMutableAttributedString()
        displayedLengths = []
        for line in lines.suffix(Self.maxLines) where matches(line) {
            let attributed = attributedLine(line)
            displayedLengths.append(attributed.length)
            text.append(attributed)
        }
        storage.setAttributedString(text)
        isFollowing = true
        textView.scrollToEndOfDocument(nil)
    }

    private func prefix(of line: Line) -> LogPrefix? {
        guard case .services(let prefixes) = source, let serviceID = line.serviceID else { return nil }
        return prefixes[serviceID]
    }

    private func matches(_ line: Line) -> Bool {
        LogStyling.matches(line.line, prefix: prefix(of: line), filter: filter)
    }

    private func attributedLine(_ line: Line) -> NSAttributedString {
        LogStyling.attributedLine(line.line, prefix: prefix(of: line), highlight: filter)
    }

    // Only touched on the main actor, and in deinit once nothing else can.
    nonisolated(unsafe) private var observers: [NSObjectProtocol] = []

    /// Whether new output scrolls into view: until the user scrolls up, and
    /// again once they scroll back to the bottom.
    private var isFollowing = true

    private func updateFollowing() {
        isFollowing = isScrolledToBottom
    }

    private var isScrolledToBottom: Bool {
        let visible = scrollView.contentView.bounds
        return visible.maxY >= textView.frame.height - 24
    }
}

/// Collects lines from a background task until the main actor takes them,
/// keeping only the newest `limit` (the view would drop older ones anyway).
nonisolated final class LineBatcher<Element: Sendable>: Sendable {
    private let limit: Int
    private let state = Mutex<(elements: [Element], isFinished: Bool)>(([], false))

    init(limit: Int) {
        self.limit = limit
    }

    func append(_ elements: [Element]) {
        state.withLock { state in
            state.elements += elements
            if state.elements.count > limit * 2 {
                state.elements.removeFirst(state.elements.count - limit)
            }
        }
    }

    func finish() {
        state.withLock { $0.isFinished = true }
    }

    /// The elements appended since the last call, and whether no more will come.
    func drain() -> (elements: [Element], isFinished: Bool) {
        state.withLock { state in
            defer { state.elements = [] }
            return (state.elements, state.isFinished)
        }
    }
}

/// A log text view whose copy leaves out the stream gutter. From the
/// keyboard it scrolls rather than moving an (invisible) insertion point:
/// ↑/↓ by line, space / ⇧space and page up/down by page, ⌘↑/⌘↓ and
/// home/end to either end.
final class LogTextView: NSTextView {
    /// Called after the keyboard scrolled the log, so following can pause or resume.
    var didScrollFromKeyboard: (() -> Void)?

    override func doCommand(by selector: Selector) {
        switch selector {
        case #selector(moveUp(_:)): scrollLines(by: -1)
        case #selector(moveDown(_:)): scrollLines(by: 1)
        case #selector(moveToBeginningOfDocument(_:)), #selector(scrollToBeginningOfDocument(_:)):
            scrollToBeginningOfDocument(nil)
        case #selector(moveToEndOfDocument(_:)), #selector(scrollToEndOfDocument(_:)):
            scrollToEndOfDocument(nil)
        case #selector(scrollPageUp(_:)), #selector(pageUp(_:)): scrollPageUp(nil)
        case #selector(scrollPageDown(_:)), #selector(pageDown(_:)): scrollPageDown(nil)
        default: return super.doCommand(by: selector)
        }
        didScrollFromKeyboard?()
    }

    override func insertText(_ string: Any, replacementRange: NSRange) {
        guard !isEditable, string as? String == " " else { return super.insertText(string, replacementRange: replacementRange) }
        NSApp.currentEvent?.modifierFlags.contains(.shift) == true ? scrollPageUp(nil) : scrollPageDown(nil)
        didScrollFromKeyboard?()
    }

    private func scrollLines(by count: CGFloat) {
        guard let scrollView = enclosingScrollView else { return }
        let font = font ?? .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        let clipView = scrollView.contentView
        let lineHeight = layoutManager?.defaultLineHeight(for: font) ?? font.pointSize
        let maxY = max(frame.height - clipView.bounds.height, 0)
        let y = min(max(clipView.bounds.origin.y + count * lineHeight, 0), maxY)
        clipView.scroll(to: NSPoint(x: clipView.bounds.origin.x, y: y))
        scrollView.reflectScrolledClipView(clipView)
    }

    /// Copies every line shown (what the filter lets through).
    @objc func copyLog(_ sender: Any?) {
        copyText(in: [NSRange(location: 0, length: textStorage?.length ?? 0)])
    }

    private func copyText(in ranges: [NSRange]) {
        guard let storage = textStorage else { return }
        var text = ""
        for range in ranges {
            storage.enumerateAttribute(.logGutter, in: range) { gutter, range, _ in
                if gutter == nil { text += storage.attributedSubstring(from: range).string }
            }
        }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}
