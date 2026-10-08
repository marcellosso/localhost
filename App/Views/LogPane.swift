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
                StateIndicator(state: status.state)
                Text(service.name).fontWeight(.medium)
                Text(status.summary)
                    .foregroundStyle(status.state == .failed ? AnyShapeStyle(.red)
                        : status.hasPortWarning ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
                    .lineLimit(1)
                Spacer()
                if let run = status.run {
                    Text("Started \(run.startedAt.formatted(date: .omitted, time: .standard))")
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
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
        .background(Color(nsColor: .textBackgroundColor))
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
                Image(systemName: "square.stack")
                    .foregroundStyle(.secondary)
                    .frame(width: 14)
                Text("All services").fontWeight(.medium)
                Text("\(running) of \(project.services.count) running")
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer()
            }
            if let logs = model.core?.logs {
                LogView(source: .services(Self.prefixes(of: project)), logs: logs, filter: model.logFilter,
                        clearCount: model.logClearCount, revealCount: 0)
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
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
                color: LogStyling.serviceColors[index % LogStyling.serviceColors.count]
            )
        }
        return prefixes
    }
}

/// The bar above a log: what it shows, then filter, copy and clear.
private struct LogHeader<Content: View>: View {
    @Environment(AppModel.self) private var model
    @FocusState private var isFilterFocused: Bool
    @ViewBuilder let content: Content

    var body: some View {
        @Bindable var model = model
        HStack(spacing: 8) {
            content
            TextField("Filter", text: $model.logFilter)
                .textFieldStyle(.roundedBorder)
                .controlSize(.small)
                .frame(width: 160)
                .focused($isFilterFocused)
                .onExitCommand { model.logFilter = "" }
                .help("Show only lines containing this text (⌘F)")
            Button("Copy Log", systemImage: "doc.on.doc") {
                NSApp.sendAction(#selector(LogTextView.copyLog(_:)), to: nil, from: nil)
            }
            .help("Copy the lines shown")
            Button("Clear Log", systemImage: "trash") { model.clearLogs() }
                .help("Clear the log view (⌃L). Log files are kept.")
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
        .font(.callout)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(Color(nsColor: .windowBackgroundColor))
        .overlay(alignment: .bottom) { Divider() }
        .onChange(of: model.logFindCount) { isFilterFocused = true }
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
                        HStack(alignment: .top, spacing: 4) {
                            Rectangle()
                                .fill(line.channel == .stderr ? Color.red.opacity(0.7) : .clear)
                                .frame(width: 2)
                            Text(ANSI.strip(line.text))
                                .lineLimit(1)
                                .truncationMode(.tail)
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
        textView.backgroundColor = .textBackgroundColor
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
        scrollView.backgroundColor = .textBackgroundColor
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
