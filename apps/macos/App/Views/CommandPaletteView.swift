import LocalhostCore
import SwiftUI

/// The command palette (⌘K): a search field over every Project, Service and
/// action. ↑/↓ (or ⌃P/⌃N) move, ↩ runs, esc or a click outside closes.
struct CommandPaletteView: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @State private var selectedID: String?
    @FocusState private var isFieldFocused: Bool

    var body: some View {
        let items = model.paletteItems()
        let results = model.paletteResults(query, in: items)
        ZStack(alignment: .top) {
            Color.black.opacity(0.12)
                .ignoresSafeArea()
                .contentShape(.rect)
                .onTapGesture(perform: close)
                .accessibilityHidden(true)
            VStack(spacing: 0) {
                field(results)
                if !results.isEmpty || !query.isEmpty {
                    Divider()
                    list(results)
                }
            }
            .frame(width: 580)
            .background(.regularMaterial, in: .rect(cornerRadius: 12))
            .background(Color(nsColor: Theme.columnSolid).opacity(0.6), in: .rect(cornerRadius: 12))
            .overlay { RoundedRectangle(cornerRadius: 12).strokeBorder(Color.primary.opacity(0.12)) }
            .shadow(color: .black.opacity(0.25), radius: 24, y: 10)
            .padding(.top, 72)
        }
        .defaultFocus($isFieldFocused, true)
        .onChange(of: query) { selectedID = results.first?.id }
        .onAppear {
            selectedID = results.first?.id
            isFieldFocused = true
        }
    }

    private func field(_ results: [PaletteItem]) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            TextField("Search", text: $query, prompt: Text("Jump to a project or service, or run an action…"))
                .labelsHidden()
                .textFieldStyle(.plain)
                .font(.system(size: 15))
                .focused($isFieldFocused)
                .onSubmit { run(results.first { $0.id == selectedID } ?? results.first) }
                .onKeyPress(.upArrow) { move(-1, in: results) }
                .onKeyPress(.downArrow) { move(1, in: results) }
                .onKeyPress(keys: ["p", "n"]) { press in
                    guard press.modifiers == .control else { return .ignored }
                    return move(press.key == "p" ? -1 : 1, in: results)
                }
                .onKeyPress(.escape) {
                    close()
                    return .handled
                }
            KeyHint("esc")
        }
        .padding(.horizontal, 14)
        .frame(height: 48)
    }

    private func list(_ results: [PaletteItem]) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 1) {
                    if results.isEmpty {
                        Text("No matches for “\(query)”")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, minHeight: 44)
                    }
                    ForEach(results) { item in
                        PaletteRow(item: item, isSelected: item.id == selectedID)
                            .id(item.id)
                            .onTapGesture { run(item) }
                            .accessibilityAction { run(item) }
                            .onHover { if $0 { selectedID = item.id } }
                    }
                }
                .padding(6)
            }
            .frame(maxHeight: 360)
            .fixedSize(horizontal: false, vertical: true)
            .onChange(of: selectedID) {
                guard let selectedID else { return }
                proxy.scrollTo(selectedID)
            }
        }
    }

    private func move(_ offset: Int, in results: [PaletteItem]) -> KeyPress.Result {
        guard !results.isEmpty else { return .handled }
        let current = results.firstIndex { $0.id == selectedID } ?? -offset
        selectedID = results[min(max(current + offset, 0), results.count - 1)].id
        return .handled
    }

    private func run(_ item: PaletteItem?) {
        guard let item else { return }
        close()
        item.action()
    }

    private func close() {
        model.isShowingPalette = false
    }
}

private struct PaletteRow: View {
    let item: PaletteItem
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 10) {
            icon
                .frame(width: 20)
            VStack(alignment: .leading, spacing: 1) {
                Text(item.title)
                    .font(.system(size: 13, weight: .medium))
                    .lineLimit(1)
                if let subtitle = item.subtitle {
                    Text(subtitle)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            Spacer(minLength: 8)
            if let shortcut = item.shortcut {
                KeyHint(shortcut)
            }
        }
        .padding(.horizontal, 10)
        .frame(minHeight: 36)
        .background(isSelected ? Theme.accent.opacity(0.16) : .clear, in: .rect(cornerRadius: 7))
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(isSelected ? [.isButton, .isSelected] : .isButton)
    }

    @ViewBuilder
    private var icon: some View {
        switch item.icon {
        case .symbol(let name):
            Image(systemName: name)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(isSelected ? Theme.accentText : .secondary)
        case .service(let color, let state):
            HStack(spacing: 3) {
                RoundedRectangle(cornerRadius: 2)
                    .fill(Color(nsColor: color))
                    .frame(width: 3, height: 14)
                StateIndicator(state: state)
            }
        }
    }
}
