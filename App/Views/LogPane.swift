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
                    FailedBanner(run: run, logs: logs) {
                        dismissedBannerRunID = run.id
                        revealCount += 1
                    }
                }
                LogView(source: .run(status.run?.id), logs: logs, filter: model.logFilter,
                        clearCount: model.logClearCount, revealCount: revealCount)
                    .overlay {
                        if status.run == nil {
                            Text("No output yet. Start the service to see its logs.")
                                .foregroundStyle(.secondary)
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
                TextField("Filter", text: $model.logFilter)
                    .textFieldStyle(.plain)
                    .focused($isFilterFocused)
                    .onExitCommand { model.logFilter = "" }
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

/// Why a Service Failed and the last 20 lines of its output.
private struct FailedBanner: View {
    let run: Run
    let logs: LogStore
    let showInLog: () -> Void
    @State private var lines: [OutputLine] = []

    static let lineLimit = 20

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Image(systemName: "exclamationmark.octagon.fill")
                    .foregroundStyle(.red)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).fontWeight(.medium)
                    if let detail {
                        Text(detail).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer()
                Button("Show in Log", action: showInLog)
                    .controlSize(.small)
            }
            if !lines.isEmpty {
                VStack(alignment: .leading, spacing: 1) {
                    ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                        // The stderr mark is an overlay: as a sibling in an HStack
                        // it would take all spare height and stretch every line.
                        Text(ANSI.strip(line.text))
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .padding(.leading, 6)
                            .overlay(alignment: .leading) {
                                if line.channel == .stderr {
                                    Rectangle().fill(Color.red.opacity(0.7)).frame(width: 2)
                                }
                            }
                    }
                }
                .font(.system(size: 11, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(10)
        .background(Color.red.opacity(0.08))
        .overlay(alignment: .bottom) { Divider() }
        .task(id: run.id) {
            // Lines may still arrive after the Service turned Failed.
            let (_, updates) = await logs.subscribe(toRun: run.id)
            lines = await logs.recentOutput(forRun: run.id, limit: Self.lineLimit)
            for await _ in updates {
                lines = await logs.recentOutput(forRun: run.id, limit: Self.lineLimit)
            }
        }
    }

    /// The typed reason if there is one, else how the command ended.
    private var title: String {
        if let failure = run.failure { return failure.message }
        return run.termination?.summary ?? "Failed"
    }

    private var detail: String? {
        run.failure == nil ? nil : run.termination?.summary
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

        scrollView.documentView = textView
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = true
        scrollView.backgroundColor = Theme.logBackground
        scrollView.borderType = .noBorder
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

    func setFilter(_ filter: String) {
        guard filter != self.filter else { return }
        self.filter = filter
        render()
    }

    func reveal(_ count: Int) {
        guard count != revealCount else { return }
        revealCount = count
        textView.scrollToEndOfDocument(nil)
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
        let followOutput = isScrolledToBottom
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

/// A log text view whose copy leaves out the stream gutter.
final class LogTextView: NSTextView {
    override func copy(_ sender: Any?) {
        copyText(in: selectedRanges.map(\.rangeValue))
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
