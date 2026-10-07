import AppKit
import LocalhostCore
import SwiftUI

/// The selected Service's output: its latest Run, live.
struct LogPane: View {
    @Environment(AppModel.self) private var model
    let service: Service

    var body: some View {
        let status = model.status(of: service)
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                StateIndicator(state: status.state)
                Text(service.name).fontWeight(.medium)
                Text(status.summary)
                    .foregroundStyle(status.state == .failed ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary))
                    .lineLimit(1)
                Spacer()
                if let run = status.run {
                    Text("Started \(run.startedAt.formatted(date: .omitted, time: .standard))")
                        .foregroundStyle(.secondary)
                }
            }
            .font(.callout)
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background(Color(nsColor: .windowBackgroundColor))
            .overlay(alignment: .bottom) { Divider() }

            if let logs = model.core?.logs {
                LogView(runID: status.run?.id, logs: logs)
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

/// An opaque, selectable, monospaced log surface backed by `NSTextView`.
/// It follows new output while scrolled to the bottom and pauses while the
/// user has scrolled up.
struct LogView: NSViewRepresentable {
    let runID: UUID?
    let logs: LogStore

    func makeCoordinator() -> LogViewController {
        LogViewController(logs: logs)
    }

    func makeNSView(context: Context) -> NSScrollView {
        context.coordinator.scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.show(runID: runID)
    }

    static func dismantleNSView(_ scrollView: NSScrollView, coordinator: LogViewController) {
        coordinator.stop()
    }
}

final class LogViewController {
    /// Lines kept in the view; matches the core's per-Run buffer.
    static let maxLines = LogStore.defaultMaxLinesPerRun

    let scrollView = NSScrollView()
    private let textView = LogTextView(frame: .zero)
    private let logs: LogStore
    private var displayedRunID: UUID?
    private var hasShownRun = false
    private var subscription: Task<Void, Never>?
    private var pending: [OutputLine] = []
    private var isFlushScheduled = false
    /// UTF-16 length of each displayed line, oldest first, for trimming.
    private var lineLengths: [Int] = []

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

    func show(runID: UUID?) {
        guard !hasShownRun || runID != displayedRunID else { return }
        hasShownRun = true
        displayedRunID = runID
        subscription?.cancel()
        pending = []
        lineLengths = []
        textView.textStorage?.setAttributedString(NSAttributedString())
        guard let runID else { return }

        subscription = Task { [weak self, logs] in
            let (lines, updates) = await logs.subscribe(toRun: runID)
            guard !Task.isCancelled else { return }
            self?.append(lines)
            for await line in updates {
                guard !Task.isCancelled else { return }
                self?.enqueue(line)
            }
            self?.flush()
        }
    }

    func stop() {
        subscription?.cancel()
    }

    /// Batches live lines so bursts of output cost one layout pass per frame or so.
    private func enqueue(_ line: OutputLine) {
        pending.append(line)
        guard !isFlushScheduled else { return }
        isFlushScheduled = true
        Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(50))
            self?.flush()
        }
    }

    private func flush() {
        isFlushScheduled = false
        guard !pending.isEmpty else { return }
        let lines = pending
        pending = []
        append(lines)
    }

    private func append(_ lines: [OutputLine]) {
        guard !lines.isEmpty, let storage = textView.textStorage else { return }
        let followOutput = isScrolledToBottom
        let text = NSMutableAttributedString()
        for line in lines {
            let attributed = LogStyling.attributedLine(line)
            lineLengths.append(attributed.length)
            text.append(attributed)
        }
        storage.beginEditing()
        storage.append(text)
        if lineLengths.count > Self.maxLines + 1_000 {
            let excess = lineLengths.count - Self.maxLines
            let length = lineLengths.prefix(excess).reduce(0, +)
            storage.deleteCharacters(in: NSRange(location: 0, length: length))
            lineLengths.removeFirst(excess)
        }
        storage.endEditing()
        if followOutput {
            textView.scrollToEndOfDocument(nil)
        }
    }

    private var isScrolledToBottom: Bool {
        let visible = scrollView.contentView.bounds
        return visible.maxY >= textView.frame.height - 24
    }
}

/// A log text view whose copy leaves out the stream gutter.
final class LogTextView: NSTextView {
    override func copy(_ sender: Any?) {
        guard let storage = textStorage else { return super.copy(sender) }
        var text = ""
        for value in selectedRanges {
            storage.enumerateAttribute(.logGutter, in: value.rangeValue) { gutter, range, _ in
                if gutter == nil { text += storage.attributedSubstring(from: range).string }
            }
        }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}
