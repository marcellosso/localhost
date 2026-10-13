import SwiftUI

// The pieces every sheet is built from (#26, round 2): a header with an icon,
// labels above fields, boxed fields with an accent focus ring, cards, chips,
// switches, and a footer with one accent button.

/// A sheet: icon, title and context, the fields (scrolling when long), then
/// a footer with an optional hint or error, Cancel and the primary action.
struct FormSheet<Content: View>: View {
    let systemImage: String
    var tint: Color = Theme.accent
    let title: String
    var subtitle: Text?
    var hint: String?
    var error: String?
    let primaryTitle: String
    var isPrimaryDisabled = false
    var width: CGFloat = 560
    let cancel: () -> Void
    let primary: () -> Void
    @ViewBuilder let content: Content

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: systemImage)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(tint)
                    .frame(width: 34, height: 34)
                    .background(tint.opacity(0.15), in: .rect(cornerRadius: 9))
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(.system(size: 16, weight: .semibold))
                        .accessibilityAddTraits(.isHeader)
                    if let subtitle {
                        subtitle
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 24)
            .padding(.top, 20)
            .padding(.bottom, 14)

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    content
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 16)
            }
            .scrollBounceBehavior(.basedOnSize)

            HStack(spacing: 8) {
                if let error {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(Color(nsColor: Theme.failed))
                        .lineLimit(2)
                } else if let hint {
                    Text(hint)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Button("Cancel", action: cancel)
                    .buttonStyle(PillButtonStyle())
                    .keyboardShortcut(.cancelAction)
                Button(primaryTitle, action: primary)
                    .buttonStyle(AccentButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(isPrimaryDisabled)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 14)
            .overlay(alignment: .top) { Divider() }
        }
        .frame(width: width)
        .frame(minHeight: 260, idealHeight: 560, maxHeight: 640)
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// A folder's path in a field box; choosing opens the folder picker.
struct FolderButton: View {
    let path: String
    let choose: () -> Void

    var body: some View {
        Button(action: choose) {
            HStack(spacing: 8) {
                Image(systemName: "folder")
                    .foregroundStyle(.secondary)
                Text(path.isEmpty ? "Choose a folder…" : (path as NSString).abbreviatingWithTildeInPath)
                    .font(.system(size: 12.5, design: .monospaced))
                    .foregroundStyle(path.isEmpty ? .secondary : .primary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                Text("Choose…")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10)
            .frame(height: 32)
            .fieldBox(isFocused: false)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

/// A field's label, with a quiet note on the right ("optional", "in logs & CLI").
struct FieldLabel: View {
    let title: String
    var note: String?

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title).font(.system(size: 12, weight: .semibold))
            Spacer(minLength: 8)
            if let note {
                Text(note).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

/// A labelled field: the label above, then the control, then an optional hint.
struct Field<Control: View>: View {
    let title: String
    var note: String?
    var hint: String?
    @ViewBuilder let control: Control

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            FieldLabel(title: title, note: note)
            control
            if let hint {
                FieldHint(hint)
            }
        }
    }
}

/// One line of help under a field.
struct FieldHint: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// A text field in a rounded box that shows the accent ring while focused.
struct BoxedTextField: View {
    let label: String
    @Binding var text: String
    var prompt: String = ""
    var isMonospaced = false
    @FocusState private var isFocused: Bool

    var body: some View {
        TextField(label, text: $text, prompt: Text(prompt))
            .labelsHidden()
            .textFieldStyle(.plain)
            .font(isMonospaced ? .body.monospaced() : .body)
            .focused($isFocused)
            .padding(.horizontal, 10)
            .frame(height: 32)
            .fieldBox(isFocused: isFocused)
    }
}

extension View {
    /// The box behind a field, with the accent focus ring.
    func fieldBox(isFocused: Bool, cornerRadius: CGFloat = 8) -> some View {
        background(Color.primary.opacity(0.05), in: .rect(cornerRadius: cornerRadius))
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius)
                    .strokeBorder(isFocused ? Theme.accent : Color.primary.opacity(0.1), lineWidth: 1)
            }
            .background {
                if isFocused {
                    RoundedRectangle(cornerRadius: cornerRadius + 3)
                        .stroke(Theme.accent.opacity(0.25), lineWidth: 3)
                        .padding(-1.5)
                }
            }
            .animation(.easeOut(duration: 0.12), value: isFocused)
    }
}

/// The command as a terminal line: its working directory (click to choose),
/// a `$`, then the command line.
struct CommandField: View {
    @Binding var workingDirectory: String
    @Binding var commandLine: String
    let chooseDirectory: () -> Void
    @FocusState private var isFocused: Bool

    var body: some View {
        HStack(spacing: 0) {
            Button(action: chooseDirectory) {
                HStack(spacing: 5) {
                    Image(systemName: "folder")
                        .font(.system(size: 11))
                    Text(workingDirectory.isEmpty || workingDirectory == "." ? "./" : "./\(workingDirectory)")
                        .lineLimit(1)
                        .truncationMode(.head)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 8, weight: .semibold))
                }
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 10)
                .frame(maxHeight: .infinity)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .frame(maxWidth: 170)
            .fixedSize(horizontal: true, vertical: false)
            .help("Working directory, relative to the project. Click to choose a folder.")
            .accessibilityLabel("Working directory")
            .accessibilityValue(workingDirectory.isEmpty || workingDirectory == "." ? "project folder" : workingDirectory)
            .accessibilityHint("Chooses a folder")
            Rectangle().fill(Color.primary.opacity(0.1)).frame(width: 1)
            Text("$")
                .accessibilityHidden(true)
                .font(.system(size: 13.5, weight: .semibold, design: .monospaced))
                .foregroundStyle(Theme.accent)
                .padding(.leading, 12)
                .padding(.trailing, 8)
            TextField("Command line", text: $commandLine, prompt: Text("pnpm dev"))
                .labelsHidden()
                .textFieldStyle(.plain)
                .font(.system(size: 13.5, design: .monospaced))
                .focused($isFocused)
                .padding(.trailing, 10)
        }
        .frame(height: 40)
        .background(Color(nsColor: Theme.logBackground), in: .rect(cornerRadius: 9))
        .fieldBox(isFocused: isFocused, cornerRadius: 9)
    }
}

/// A group of rows on a quiet card, e.g. the variables table.
struct FormCard<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            content
        }
        .background(Color.primary.opacity(0.03), in: .rect(cornerRadius: 10))
        .overlay { RoundedRectangle(cornerRadius: 10).strokeBorder(Color.primary.opacity(0.08)) }
    }
}

/// A toggle drawn as a chip in a colour, e.g. a Dependency in its Service's colour.
struct ChipToggle: View {
    let title: String
    let color: Color
    @Binding var isOn: Bool

    var body: some View {
        Button { isOn.toggle() } label: {
            HStack(spacing: 6) {
                RoundedRectangle(cornerRadius: 2)
                    .fill(color)
                    .frame(width: 7, height: 7)
                Text(title)
                if isOn {
                    Image(systemName: "checkmark")
                        .font(.system(size: 9, weight: .bold))
                }
            }
            .font(.system(size: 12, weight: isOn ? .semibold : .regular))
            .padding(.leading, 8)
            .padding(.trailing, 10)
            .frame(height: 26)
            .background(isOn ? color.opacity(0.15) : .clear, in: .capsule)
            .overlay { Capsule().strokeBorder(isOn ? color.opacity(0.5) : Color.primary.opacity(0.12)) }
            .contentShape(.capsule)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isOn ? .isSelected : [])
    }
}

/// A setting with a switch on the right.
struct SwitchRow: View {
    let title: String
    var detail: String?
    @Binding var isOn: Bool

    var body: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 12, weight: .semibold))
                if let detail {
                    FieldHint(detail)
                }
            }
            Spacer(minLength: 0)
            Toggle(title, isOn: $isOn)
                .labelsHidden()
                .toggleStyle(.switch)
                .controlSize(.small)
                .tint(Theme.accent)
        }
    }
}

/// A simple wrapping layout for chips. Nonisolated: SwiftUI can measure a
/// layout off the main thread (e.g. while a sheet animates in).
nonisolated struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(proposal: proposal, subviews: subviews)
        let height = rows.last.map { $0.y + $0.height } ?? 0
        return CGSize(width: proposal.width ?? rows.map(\.width).max() ?? 0, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for row in arrange(proposal: proposal, subviews: subviews) {
            var x = bounds.minX
            for index in row.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: bounds.minY + row.y), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
        }
    }

    private struct Row {
        var indices: [Int] = []
        var y: CGFloat = 0
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func arrange(proposal: ProposedViewSize, subviews: Subviews) -> [Row] {
        let maxWidth = proposal.width ?? .infinity
        var rows: [Row] = [Row()]
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            if !rows[rows.count - 1].indices.isEmpty, rows[rows.count - 1].width + spacing + size.width > maxWidth {
                let last = rows[rows.count - 1]
                rows.append(Row(y: last.y + last.height + spacing))
            }
            var row = rows[rows.count - 1]
            row.width += (row.indices.isEmpty ? 0 : spacing) + size.width
            row.height = max(row.height, size.height)
            row.indices.append(index)
            rows[rows.count - 1] = row
        }
        return rows
    }
}
