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
    /// The drafts it starts after; only the checked ones are saved.
    var startsAfter: [UUID] = []
    /// The drafts merged into this one, to split it again.
    var mergedFrom: [SuggestionDraft] = []

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

    /// The draft as the Suggestion it now is, with the user's edits, to merge it.
    var editedSuggestion: ServiceSuggestion {
        var edited = suggestion
        edited.name = name
        edited.key = (try? Key(key.trimmingCharacters(in: .whitespaces))) ?? suggestion.key
        edited.workingDirectory = workingDirectory
        edited.commandLine = commandLine
        edited.corePort = UInt16(port.trimmingCharacters(in: .whitespaces))
        edited.portCanChange = portCanChange
        return edited
    }

    /// The checked drafts as Services to add, each starting after the checked
    /// drafts it picked, or a message saying what to fix.
    static func newServices(from drafts: [SuggestionDraft]) -> Result<[NewService], ReviewError> {
        let checked = drafts.filter(\.isChecked)
        let keys = Dictionary(uniqueKeysWithValues: checked.map { ($0.id, $0.key.trimmingCharacters(in: .whitespaces)) })
        var services: [NewService] = []
        for draft in checked {
            let trimmedPort = draft.port.trimmingCharacters(in: .whitespaces)
            let port = Int(trimmedPort)
            guard trimmedPort.isEmpty || port != nil else {
                return .failure(ReviewError(message: "The port of “\(draft.name)” must be a number, e.g. 3000."))
            }
            do {
                var dependencyKeys: [Key] = []
                for raw in draft.startsAfter.compactMap({ keys[$0] }) {
                    dependencyKeys.append(try Key(raw))
                }
                services.append(NewService(
                    name: draft.name, key: try Key(draft.key.trimmingCharacters(in: .whitespaces)),
                    workingDirectory: draft.workingDirectory, commandLine: draft.commandLine,
                    port: port, portCanChange: draft.portCanChange, dependencyKeys: dependencyKeys
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

extension [SuggestionDraft] {
    /// The drafts `ids` replaced with one, checked, for `merged` (the core's
    /// merge of them), where the first of them was. Drafts that started after
    /// any of them start after the merged one.
    func merging(_ ids: [UUID], into merged: ServiceSuggestion) -> [SuggestionDraft] {
        let parts = filter { ids.contains($0.id) }
        guard let first = parts.first, let position = firstIndex(where: { $0.id == first.id }) else { return self }
        var mergedSuggestion = merged
        mergedSuggestion.isRecommended = true
        var draft = SuggestionDraft(mergedSuggestion)
        draft.isChecked = true
        draft.startsAfter = Self.unique(parts.flatMap(\.startsAfter)).filter { !ids.contains($0) }
        draft.mergedFrom = parts
        var result = map { other in
            var other = other
            other.startsAfter = Self.replacing(other.startsAfter, ids, with: [draft.id])
            return other
        }
        result.insert(draft, at: position)
        return result.filter { !ids.contains($0.id) }
    }

    /// The drafts running one Compose service each that `draft` (running
    /// several) could split into.
    func containerDrafts(of draft: SuggestionDraft) -> [SuggestionDraft] {
        let services = draft.suggestion.composeServices
        guard services.count > 1 else { return [] }
        return filter { other in
            other.id != draft.id && other.suggestion.composeServices.count == 1
                && services.contains(other.suggestion.composeServices[0])
                && other.suggestion.foundIn == draft.suggestion.foundIn
                && other.workingDirectory == draft.workingDirectory
        }
    }

    /// "Undo merge" or "One Service per container", when `draft` can be split.
    func splitTitle(of draft: SuggestionDraft) -> String? {
        if !draft.mergedFrom.isEmpty { return "Undo merge" }
        return containerDrafts(of: draft).isEmpty ? nil : "One Service per container"
    }

    /// Undoes a merge, or checks the one-per-container drafts instead of one
    /// running the whole Compose file (which they start after what it started
    /// after). Drafts that started after it start after all of them.
    func splitting(_ id: UUID) -> [SuggestionDraft] {
        guard let draft = first(where: { $0.id == id }) else { return self }
        if !draft.mergedFrom.isEmpty {
            let parts = draft.mergedFrom.map { part in
                var part = part
                part.isChecked = true
                return part
            }
            let partIDs = parts.map(\.id)
            return flatMap { other -> [SuggestionDraft] in
                if other.id == id { return parts }
                var other = other
                other.startsAfter = Self.replacing(other.startsAfter, [id], with: partIDs)
                return [other]
            }
        }
        let partIDs = containerDrafts(of: draft).map(\.id)
        guard !partIDs.isEmpty else { return self }
        return map { other in
            var other = other
            if other.id == id {
                other.isChecked = false
                return other
            }
            other.startsAfter = Self.replacing(other.startsAfter, [id], with: partIDs)
            if partIDs.contains(other.id) {
                other.isChecked = true
                other.startsAfter = Self.unique(other.startsAfter + draft.startsAfter)
            }
            return other
        }
    }

    /// Whether `draft` starts after `other`, directly or through other checked
    /// drafts: then `other` can't start after it.
    func startsAfter(_ draft: SuggestionDraft, _ other: SuggestionDraft) -> Bool {
        let byID = Dictionary(uniqueKeysWithValues: filter(\.isChecked).map { ($0.id, $0) })
        var seen: Set<UUID> = []
        var queue = draft.startsAfter
        while let id = queue.popLast() {
            if id == other.id { return true }
            guard seen.insert(id).inserted, let next = byID[id] else { continue }
            queue += next.startsAfter
        }
        return false
    }

    private static func unique(_ ids: [UUID]) -> [UUID] {
        var seen: Set<UUID> = []
        return ids.filter { seen.insert($0).inserted }
    }

    /// `ids` with any of `removed` replaced by `added`, once each.
    private static func replacing(_ ids: [UUID], _ removed: [UUID], with added: [UUID]) -> [UUID] {
        guard ids.contains(where: removed.contains) else { return ids }
        return unique(ids.flatMap { removed.contains($0) ? added : [$0] })
    }
}

/// The review step after scanning a folder: the Suggestions that are likely
/// Services (checked) and, folded away, the rest of what was found. Each row
/// can be checked, unchecked, and opened to edit it, choose what it starts
/// after, or split it; several can be merged into one. Nothing is saved here.
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
    /// The drafts picked to merge, while picking them.
    @State private var merging: [UUID]?

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
                    let showsOthers = isShowingOthers || likely.isEmpty || merging != nil
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
                                    .rotationEffect(.degrees(showsOthers ? 90 : 0))
                                Text("\(others.count) more found")
                                Text(othersDetail(others))
                                    .foregroundStyle(.tertiary)
                            }
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .disabled(likely.isEmpty || merging != nil)
                        if showsOthers {
                            card(others)
                        }
                    }
                    if drafts.count > 1 {
                        mergeBar
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
        if let merging { return "\(merging.count) to merge" }
        let count = drafts.filter(\.isChecked).count
        return count == 0 ? "none selected" : "\(count) selected · edit before adding"
    }

    private func othersDetail(_ others: [Binding<SuggestionDraft>]) -> String {
        let hasContainers = others.contains { $0.wrappedValue.suggestion.composeServices.count == 1 }
        return hasContainers ? "· one per container, build steps, other scripts" : "· build steps, other scripts"
    }

    @ViewBuilder private var mergeBar: some View {
        HStack(spacing: 8) {
            if let merging {
                Text("Pick the ones to run as one Service.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Cancel") { self.merging = nil }
                Button("Merge into one Service") { merge(merging) }
                    .disabled(merging.count < 2)
            } else {
                Button("Merge…") { merging = [] }
                Text("Run several as one Service, e.g. a Compose file")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
            }
        }
        .controlSize(.small)
    }

    private func merge(_ ids: [UUID]) {
        let parts = drafts.filter { ids.contains($0.id) }.map(\.editedSuggestion)
        guard let merged = ServiceSuggestion.merged(parts) else { return }
        let before = Set(drafts.map(\.id))
        drafts = drafts.merging(ids, into: merged)
        merging = nil
        expandedID = drafts.first { !before.contains($0.id) }?.id
    }

    private func split(_ draft: SuggestionDraft) {
        if draft.mergedFrom.isEmpty { isShowingOthers = true }
        drafts = drafts.splitting(draft.id)
        expandedID = nil
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
                    isExpanded: Binding { merging == nil && expandedID == draft.id } set: { expandedID = $0 ? draft.id : nil },
                    chooseFolder: { choosingFolderFor = draft.id },
                    startsAfterOptions: startsAfterOptions(for: draft),
                    splitTitle: drafts.splitTitle(of: draft),
                    split: { split(draft) },
                    isPickedToMerge: merging.map { ids in Binding { ids.contains(draft.id) } set: { pick(draft.id, $0) } }
                )
            }
        }
    }

    private func pick(_ id: UUID, _ isPicked: Bool) {
        guard var ids = merging else { return }
        if isPicked { ids.append(id) } else { ids.removeAll { $0 == id } }
        merging = ids
    }

    private func startsAfterOptions(for draft: SuggestionDraft) -> [SuggestionRow.Option] {
        drafts.filter { $0.isChecked && $0.id != draft.id }.map { other in
            SuggestionRow.Option(draft: other, color: color(of: other), isCycle: drafts.startsAfter(other, draft))
        }
    }

    /// A checked draft's colour is the one its Service will get; unchecked ones are grey.
    private func color(of draft: SuggestionDraft) -> NSColor {
        guard let index = drafts.filter(\.isChecked).firstIndex(where: { $0.id == draft.id }) else { return Theme.stopped }
        return Theme.serviceColor(at: existingServiceCount + index)
    }
}

/// One Suggestion: a checkbox, its name, Key, command and Port, and when
/// opened, fields to edit it, what it starts after and where it was found.
private struct SuggestionRow: View {
    /// Another checked draft it can start after; a cycle when that one
    /// already starts after it.
    struct Option {
        let draft: SuggestionDraft
        let color: NSColor
        let isCycle: Bool
    }

    @Binding var draft: SuggestionDraft
    let color: NSColor
    @Binding var isExpanded: Bool
    let chooseFolder: () -> Void
    let startsAfterOptions: [Option]
    /// "Undo merge" or "One Service per container", when it can be split.
    let splitTitle: String?
    let split: () -> Void
    /// While picking drafts to merge, whether this one is picked.
    let isPickedToMerge: Binding<Bool>?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                if let isPickedToMerge {
                    Toggle("Merge \(draft.name)", isOn: isPickedToMerge)
                        .toggleStyle(.checkbox)
                        .labelsHidden()
                } else {
                    Toggle("Add \(draft.name)", isOn: $draft.isChecked)
                        .toggleStyle(.checkbox)
                        .labelsHidden()
                }
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
                        if !startsAfterNames.isEmpty {
                            Text("after \(startsAfterNames)")
                                .font(.system(size: 11))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                        }
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
                if isPickedToMerge == nil {
                    // A button, so the keyboard can open the editor too.
                    Button(isExpanded ? "Close Editor" : "Edit \(draft.name)", systemImage: "chevron.down") { toggleEditor() }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .rotationEffect(.degrees(isExpanded ? 180 : 0))
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .contentShape(.rect)
            .onTapGesture {
                if let isPickedToMerge { isPickedToMerge.wrappedValue.toggle() } else { toggleEditor() }
            }
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
        .opacity((isPickedToMerge?.wrappedValue ?? draft.isChecked) ? 1 : 0.65)
    }

    private func toggleEditor() {
        withAnimation(.motion(.easeOut(duration: 0.15))) { isExpanded.toggle() }
    }

    private var commandSummary: String {
        let folder = draft.workingDirectory == "." ? "./" : "./\(draft.workingDirectory)"
        return "\(folder) $ \(draft.commandLine)"
    }

    private var startsAfterNames: String {
        guard draft.isChecked else { return "" }
        return startsAfterOptions.filter { draft.startsAfter.contains($0.draft.id) }.map(\.draft.name).joined(separator: ", ")
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
            if draft.isChecked, !startsAfterOptions.isEmpty {
                Field(title: "Starts after", note: "waits until they’re running", hint: startsAfterHint) {
                    FlowLayout {
                        ForEach(startsAfterOptions, id: \.draft.id) { option in
                            dependencyChip(option)
                        }
                    }
                }
            }
            HStack(spacing: 10) {
                (Text("Found in ") + Text(draft.suggestion.foundIn).monospaced() + Text(" · ") + Text(draft.suggestion.detail).monospaced())
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let splitTitle {
                    Button(splitTitle, action: split)
                        .controlSize(.small)
                }
            }
        }
    }

    private var startsAfterHint: String? {
        startsAfterOptions.contains(where: \.isCycle)
            ? "Services that already start after this one can’t be chosen: that would be a cycle."
            : nil
    }

    private func dependencyChip(_ option: Option) -> some View {
        let isOn = Binding {
            draft.startsAfter.contains(option.draft.id)
        } set: { isOn in
            if isOn { draft.startsAfter.append(option.draft.id) } else { draft.startsAfter.removeAll { $0 == option.draft.id } }
        }
        let isBlocked = option.isCycle && !isOn.wrappedValue
        return ChipToggle(title: option.draft.name, color: Color(nsColor: option.color), isOn: isOn)
            .disabled(isBlocked)
            .opacity(isBlocked ? 0.4 : 1)
            .help(isBlocked ? "\(option.draft.name) already starts after \(draft.name)." : option.draft.key)
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
