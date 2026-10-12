import LocalhostCore
import SwiftUI
import UniformTypeIdentifiers

/// A Suggestion as reviewed before saving: checked or not, and edited.
struct SuggestionDraft: Identifiable {
    let id = UUID()
    let suggestion: ServiceSuggestion
    var isChecked: Bool
    var name: String
    var key: String
    /// Once the user edits the key it stops following the name.
    var isKeyEdited = false
    var workingDirectory: String
    var commandLine: String
    var port: String
    var portCanChange: Bool

    init(_ suggestion: ServiceSuggestion) {
        self.suggestion = suggestion
        isChecked = suggestion.isRecommended
        name = suggestion.name
        key = suggestion.key.rawValue
        workingDirectory = suggestion.workingDirectory
        commandLine = suggestion.commandLine
        port = suggestion.port.map(String.init) ?? ""
        portCanChange = suggestion.portCanChange
    }

    /// The checked drafts as Services to add, or a message saying what to fix.
    static func newServices(from drafts: [SuggestionDraft]) -> Result<[NewService], ReviewError> {
        var services: [NewService] = []
        for draft in drafts where draft.isChecked {
            let trimmedPort = draft.port.trimmingCharacters(in: .whitespaces)
            let port = Int(trimmedPort)
            guard trimmedPort.isEmpty || port != nil else {
                return .failure(ReviewError(message: "The port of “\(draft.name)” must be a number, e.g. 3000."))
            }
            do {
                services.append(NewService(
                    name: draft.name, key: try Key(draft.key.trimmingCharacters(in: .whitespaces)),
                    workingDirectory: draft.workingDirectory, commandLine: draft.commandLine,
                    port: port, portCanChange: draft.portCanChange
                ))
            } catch {
                return .failure(ReviewError(message: error.message))
            }
        }
        return .success(services)
    }

    struct ReviewError: Error {
        let message: String
    }
}

/// The review step after scanning a folder: the Suggestions that are likely
/// Services (checked) and, folded away, the rest of what was found. Each row
/// can be checked, unchecked, and opened to edit it. Nothing is saved here.
struct SuggestionReview: View {
    @Binding var drafts: [SuggestionDraft]
    let isScanning: Bool
    /// The Project folder, for choosing working directories.
    let projectPath: String
    /// The number of Services the Project already has, so new ones get the
    /// colours they'll keep.
    var existingServiceCount = 0
    let emptyMessage: String

    @State private var expandedID: UUID?
    @State private var isShowingOthers = false
    @State private var choosingFolderFor: UUID?

    var body: some View {
        Field(title: "Services", note: isScanning || drafts.isEmpty ? nil : selectionNote) {
            if isScanning {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Looking for package.json, Compose, Python and Cargo files…")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, minHeight: 44)
            } else if drafts.isEmpty {
                FieldHint(emptyMessage)
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    let likely = rows(isRecommended: true)
                    let others = rows(isRecommended: false)
                    if !likely.isEmpty {
                        card(likely)
                    }
                    if !others.isEmpty {
                        Button {
                            withAnimation(.motion(.easeOut(duration: 0.15))) { isShowingOthers.toggle() }
                        } label: {
                            HStack(spacing: 5) {
                                Image(systemName: "chevron.right")
                                    .font(.system(size: 9, weight: .semibold))
                                    .rotationEffect(.degrees(isShowingOthers || likely.isEmpty ? 90 : 0))
                                Text("\(others.count) more found")
                                Text("· build steps, other scripts")
                                    .foregroundStyle(.tertiary)
                            }
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .disabled(likely.isEmpty)
                        if isShowingOthers || likely.isEmpty {
                            card(others)
                        }
                    }
                }
            }
        }
        .fileImporter(isPresented: Binding { choosingFolderFor != nil } set: { if !$0 { choosingFolderFor = nil } },
                      allowedContentTypes: [.folder]) { result in
            guard case .success(let url) = result, let id = choosingFolderFor,
                  let index = drafts.firstIndex(where: { $0.id == id }) else { return }
            drafts[index].workingDirectory = ServiceForm.relativePath(of: url.path, to: projectPath)
        }
    }

    private var selectionNote: String {
        let count = drafts.filter(\.isChecked).count
        return count == 0 ? "none selected" : "\(count) selected · edit before adding"
    }

    private func rows(isRecommended: Bool) -> [Binding<SuggestionDraft>] {
        $drafts.filter { $0.wrappedValue.suggestion.isRecommended == isRecommended }
    }

    private func card(_ rows: [Binding<SuggestionDraft>]) -> some View {
        FormCard {
            ForEach(Array(rows.enumerated()), id: \.element.wrappedValue.id) { offset, $draft in
                if offset > 0 { Divider().padding(.leading, 38) }
                SuggestionRow(
                    draft: $draft,
                    color: color(of: draft),
                    isExpanded: Binding { expandedID == draft.id } set: { expandedID = $0 ? draft.id : nil },
                    chooseFolder: { choosingFolderFor = draft.id }
                )
            }
        }
    }

    /// A checked draft's colour is the one its Service will get; unchecked ones are grey.
    private func color(of draft: SuggestionDraft) -> NSColor {
        guard let index = drafts.filter(\.isChecked).firstIndex(where: { $0.id == draft.id }) else { return Theme.stopped }
        return Theme.serviceColor(at: existingServiceCount + index)
    }
}

/// One Suggestion: a checkbox, its name, Key, command and Port, and when
/// opened, fields to edit it and where it was found.
private struct SuggestionRow: View {
    @Binding var draft: SuggestionDraft
    let color: NSColor
    @Binding var isExpanded: Bool
    let chooseFolder: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Toggle("Add \(draft.name)", isOn: $draft.isChecked)
                    .toggleStyle(.checkbox)
                    .labelsHidden()
                RoundedRectangle(cornerRadius: 2)
                    .fill(Color(nsColor: color))
                    .frame(width: 7, height: 7)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text(draft.name)
                            .font(.system(size: 12.5, weight: .semibold))
                            .lineLimit(1)
                        Text(draft.key)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Text(commandSummary)
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 8)
                if let port = Int(draft.port) {
                    PortPill(port: port, color: color)
                }
                // A button, so the keyboard can open the editor too.
                Button(isExpanded ? "Close Editor" : "Edit \(draft.name)", systemImage: "chevron.down") { toggleEditor() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.plain)
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .rotationEffect(.degrees(isExpanded ? 180 : 0))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .contentShape(.rect)
            .onTapGesture(perform: toggleEditor)
            .help("From \(draft.suggestion.foundIn) · \(draft.suggestion.detail)")
            .accessibilityElement(children: .contain)

            if isExpanded {
                editor
                    .padding(.horizontal, 12)
                    .padding(.leading, 26)
                    .padding(.bottom, 12)
                    .transition(.opacity)
            }
        }
        .opacity(draft.isChecked ? 1 : 0.65)
    }

    private func toggleEditor() {
        withAnimation(.motion(.easeOut(duration: 0.15))) { isExpanded.toggle() }
    }

    private var commandSummary: String {
        let folder = draft.workingDirectory == "." ? "./" : "./\(draft.workingDirectory)"
        return "\(folder) $ \(draft.commandLine)"
    }

    private var editor: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                Field(title: "Name") {
                    BoxedTextField(label: "Name", text: $draft.name, prompt: "Web")
                }
                Field(title: "Key") {
                    BoxedTextField(label: "Key", text: keyBinding, prompt: "web", isMonospaced: true)
                }
                .frame(width: 140)
                Field(title: "Port") {
                    BoxedTextField(label: "Port", text: $draft.port, prompt: "—", isMonospaced: true)
                }
                .frame(width: 80)
            }
            .onChange(of: draft.name) {
                if !draft.isKeyEdited { draft.key = Key(suggestedFrom: draft.name)?.rawValue ?? "" }
            }
            Field(title: "Command") {
                CommandField(workingDirectory: $draft.workingDirectory, commandLine: $draft.commandLine, chooseDirectory: chooseFolder)
            }
            if !draft.port.isEmpty {
                SwitchRow(title: "Port can change",
                          detail: draft.portCanChange
                              ? "If the port is in use, a run may start on the next free one (passed as $PORT)."
                              : "The port is fixed by the command or a Compose file, so it can’t move.",
                          isOn: $draft.portCanChange)
            }
            (Text("Found in ") + Text(draft.suggestion.foundIn).monospaced() + Text(" · ") + Text(draft.suggestion.detail).monospaced())
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }

    private var keyBinding: Binding<String> {
        Binding { draft.key } set: { newValue in
            draft.key = newValue
            draft.isKeyEdited = true
        }
    }
}

/// Scans an existing Project's folder again and adds the Services the user
/// picks from what it finds.
struct DetectServicesForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let project: Project

    @State private var drafts: [SuggestionDraft] = []
    @State private var isScanning = true
    @State private var error: String?
    @State private var isSaving = false

    var body: some View {
        let count = drafts.filter(\.isChecked).count
        FormSheet(
            systemImage: "sparkle.magnifyingglass",
            title: "Detect Services",
            subtitle: Text("in **\(project.name)** · ") + Text((project.path as NSString).abbreviatingWithTildeInPath).monospaced(),
            hint: count == 0 ? nil : "↩ to add",
            error: error,
            primaryTitle: count == 0 ? "Add Services" : "Add \(count) Service\(count == 1 ? "" : "s")",
            isPrimaryDisabled: count == 0 || isSaving,
            width: 640,
            cancel: { dismiss() },
            primary: { Task { await save() } }
        ) {
            SuggestionReview(
                drafts: $drafts, isScanning: isScanning, projectPath: project.path,
                existingServiceCount: project.services.count,
                emptyMessage: "Nothing new found. Services this project already runs aren’t suggested again."
            )
        }
        .task {
            drafts = await model.suggestions(for: project).map(SuggestionDraft.init)
            isScanning = false
        }
    }

    private func save() async {
        switch SuggestionDraft.newServices(from: drafts) {
        case .failure(let failure):
            error = failure.message
        case .success(let services):
            isSaving = true
            defer { isSaving = false }
            do {
                try await model.addServices(services, to: project.id)
                dismiss()
            } catch {
                self.error = error.message
            }
        }
    }
}
