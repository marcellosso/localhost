import LocalhostCore
import SwiftUI

/// How stats are sampled: only while a stats popover is open, every 1.5
/// seconds, and the sparklines keep the last minute (ADR-0012).
enum StatsSampling {
    static let interval = Duration.milliseconds(1500)
    static let historyLength = 40
}

/// One Run's samples since its stats were opened.
struct StatsHistory {
    private(set) var samples: [RunStats] = []
    /// The most memory seen since then.
    private(set) var peakMemory: UInt64 = 0

    var latest: RunStats? { samples.last }

    /// Adds a sample; a sample of another Run starts over, `nil` (no active Run) clears it.
    mutating func append(_ stats: RunStats?) {
        guard let stats else {
            self = StatsHistory()
            return
        }
        if samples.last?.runId != stats.runId {
            self = StatsHistory()
        }
        samples.append(stats)
        if samples.count > StatsSampling.historyLength {
            samples.removeFirst(samples.count - StatsSampling.historyLength)
        }
        peakMemory = max(peakMemory, stats.memoryBytes)
    }
}

extension AppModel {
    /// What the Service's active Run uses now, `nil` without one (or if it can't be measured).
    func runStats(of serviceID: UUID, previous: RunStats?) async -> RunStats? {
        guard let services = core?.services else { return nil }
        return try? await services.runStats(for: serviceID, previous: previous)
    }
}

enum StatsFormat {
    /// CPU use where 100% is one core; "–" until there are two samples.
    static func cpu(_ percent: Double?) -> String {
        guard let percent else { return "–" }
        return percent.formatted(.number.precision(.fractionLength(percent < 10 ? 1 : 0))) + "%"
    }

    static func memory(_ bytes: UInt64) -> String {
        Int64(clamping: bytes).formatted(.byteCount(style: .memory))
    }
}

/// The gauge button in a running Service's log header: its CPU and memory in a popover.
struct ServiceStatsButton: View {
    let service: Service
    @State private var isPresented = false

    var body: some View {
        Button("CPU and Memory", systemImage: "gauge.medium") { isPresented.toggle() }
            .labelStyle(.iconOnly)
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .help("CPU and memory of \(service.name)")
            .accessibilityLabel("Show CPU and Memory of \(service.name)")
            .popover(isPresented: $isPresented, arrowEdge: .bottom) {
                ServiceStatsView(service: service)
            }
    }
}

/// One Service's CPU and memory with sparklines, sampled while it is shown.
private struct ServiceStatsView: View {
    @Environment(AppModel.self) private var model
    let service: Service
    @State private var history = StatsHistory()
    @State private var hasSampled = false

    var body: some View {
        let color = Color(nsColor: model.color(of: service))
        VStack(alignment: .leading, spacing: 12) {
            Text(service.name).fontWeight(.semibold)
            if let latest = history.latest {
                StatRow(title: "CPU", value: StatsFormat.cpu(latest.cpuPercent),
                        values: history.samples.compactMap(\.cpuPercent), floor: 100, color: color)
                    .help("100% is one core fully busy")
                StatRow(title: "Memory", value: StatsFormat.memory(latest.memoryBytes),
                        values: history.samples.map { Double($0.memoryBytes) }, floor: 0, color: color)
                    .help("Physical footprint, as Activity Monitor shows it, added up over the Run's processes")
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
                    GridRow {
                        Text("Processes").foregroundStyle(.secondary)
                        Text(latest.processCount, format: .number).monospacedDigit()
                    }
                    GridRow {
                        Text("Peak memory").foregroundStyle(.secondary)
                        Text(StatsFormat.memory(history.peakMemory)).monospacedDigit()
                    }
                    .help("The most memory used since this was opened")
                }
                .font(.callout)
            } else {
                Text(hasSampled ? "Not running." : "Measuring…")
                    .foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .frame(width: 250, alignment: .leading)
        .task {
            while !Task.isCancelled {
                history.append(await model.runStats(of: service.id, previous: history.latest))
                hasSampled = true
                try? await Task.sleep(for: StatsSampling.interval)
            }
        }
    }
}

/// A label, its current value, and a sparkline of its recent values.
private struct StatRow: View {
    let title: String
    let value: String
    let values: [Double]
    /// The smallest top of the sparkline's scale, so a quiet Run doesn't look busy.
    let floor: Double
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(title).foregroundStyle(.secondary)
                Spacer()
                Text(value).fontWeight(.medium).monospacedDigit()
            }
            .font(.callout)
            Sparkline(values: values, top: max(floor, (values.max() ?? 0) * 1.1), color: color)
                .frame(height: 28)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(value)
    }
}

/// Recent values as a line, the newest at the right edge, filling leftwards
/// up to `StatsSampling.historyLength` values.
private struct Sparkline: View {
    let values: [Double]
    let top: Double
    let color: Color

    var body: some View {
        Canvas { context, size in
            guard values.count > 1, top > 0 else { return }
            let step = size.width / CGFloat(StatsSampling.historyLength - 1)
            let points = values.enumerated().map { index, value in
                CGPoint(x: size.width - CGFloat(values.count - 1 - index) * step,
                        y: size.height * (1 - CGFloat(min(value, top) / top)))
            }
            var line = Path()
            line.addLines(points)
            var area = line
            area.addLine(to: CGPoint(x: points[points.count - 1].x, y: size.height))
            area.addLine(to: CGPoint(x: points[0].x, y: size.height))
            area.closeSubpath()
            context.fill(area, with: .color(color.opacity(0.15)))
            context.stroke(line, with: .color(color), style: StrokeStyle(lineWidth: 1.5, lineJoin: .round))
        }
        .background(Color.primary.opacity(0.04), in: .rect(cornerRadius: 4))
        .accessibilityHidden(true)
    }
}

/// "Stats…" in a Project's `⋯` menu: each running Service's CPU and memory, and the total.
struct ProjectStatsView: View {
    @Environment(AppModel.self) private var model
    let project: Project
    @State private var histories: [UUID: StatsHistory] = [:]

    var body: some View {
        let measured = project.services.compactMap { service in
            histories[service.id]?.latest.map { (service: service, stats: $0) }
        }
        VStack(alignment: .leading, spacing: 10) {
            Text(project.name).fontWeight(.semibold)
            if measured.isEmpty {
                Text("No services running.").foregroundStyle(.secondary)
            } else {
                Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 6) {
                    GridRow {
                        Text("Service")
                        Text("CPU").gridColumnAlignment(.trailing)
                        Text("Memory").gridColumnAlignment(.trailing)
                    }
                    .foregroundStyle(.secondary)
                    Divider().gridCellUnsizedAxes(.horizontal)
                    ForEach(measured, id: \.service.id) { service, stats in
                        GridRow {
                            HStack(spacing: 6) {
                                RoundedRectangle(cornerRadius: 2)
                                    .fill(Color(nsColor: model.color(of: service)))
                                    .frame(width: 8, height: 8)
                                Text(service.name).lineLimit(1)
                            }
                            Text(StatsFormat.cpu(stats.cpuPercent)).monospacedDigit()
                            Text(StatsFormat.memory(stats.memoryBytes)).monospacedDigit()
                        }
                        .accessibilityElement(children: .combine)
                    }
                    Divider().gridCellUnsizedAxes(.horizontal)
                    GridRow {
                        Text("Total").fontWeight(.medium)
                        Text(StatsFormat.cpu(total(measured.map(\.stats.cpuPercent)))).monospacedDigit()
                        Text(StatsFormat.memory(measured.reduce(0) { $0 + $1.stats.memoryBytes })).monospacedDigit()
                    }
                    .accessibilityElement(children: .combine)
                }
                .font(.callout)
            }
        }
        .padding(14)
        .frame(minWidth: 260, alignment: .leading)
        .task {
            while !Task.isCancelled {
                await sample()
                try? await Task.sleep(for: StatsSampling.interval)
            }
        }
    }

    /// Samples every active Service of the Project; the others are dropped.
    private func sample() async {
        let active = project.services.filter { model.status(of: $0).state.isActive }.map(\.id)
        var sampled: [UUID: StatsHistory] = [:]
        for serviceID in active {
            var history = histories[serviceID] ?? StatsHistory()
            history.append(await model.runStats(of: serviceID, previous: history.latest))
            sampled[serviceID] = history
        }
        histories = sampled
    }

    /// The sum of the CPU use, once every Service has it.
    private func total(_ percents: [Double?]) -> Double? {
        percents.contains(nil) ? nil : percents.reduce(0) { $0 + ($1 ?? 0) }
    }
}
