import LocalhostCore
import SwiftUI

/// Chooses a folder (or a Project file) on another Machine, which this Mac's
/// open panel can't browse: a path to type, the folders above it as a
/// breadcrumb, Up, and what the folder holds, listed by the Machine. Choose
/// picks the selected folder, else the one shown; for a Project file, the
/// selected file. Paths are the Machine's, shown as they are.
struct RemoteFolderPicker: View {
    enum Mode {
        case folder
        /// Folders and Project files (`.yaml`, `.yml`); other files are hidden.
        case projectFile
    }

    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let machineID: UUID
    var mode: Mode = .folder
    /// Where it opens; `nil` for the home folder there.
    var startingAt: String?
    let choose: (String) -> Void

    /// The folder shown, kept while another can't be opened.
    @State private var listing: DirectoryListing?
    @State private var typedPath = ""
    /// The selected entry's name.
    @State private var selection: String?
    @State private var showsHidden = false
    /// Why the folder asked for can't be shown, as the Machine says.
    @State private var error: String?
    @State private var isLoading = false
    /// The latest request, so an answer to an earlier one is ignored.
    @State private var request = UUID()
    @FocusState private var isPathFocused: Bool
    @FocusState private var isListFocused: Bool

    var body: some View {
        let machine = model.machineName(.machine(machineID)) ?? "the Machine"
        FormSheet(
            systemImage: mode == .folder ? "folder" : "doc.text",
            title: mode == .folder ? "Choose a Folder on \(machine)" : "Choose a Project File on \(machine)",
            hint: mode == .folder ? "Double-click or ↩ opens a folder." : "Shows folders and Project files (.yaml, .yml).",
            primaryTitle: "Choose",
            isPrimaryDisabled: chosenPath == nil,
            isPrimaryDefault: isChooseDefault,
            width: 560,
            cancel: { dismiss() },
            primary: {
                if let path = chosenPath { pick(path) }
            }
        ) {
            pathBar
            if !crumbs.isEmpty {
                breadcrumb
            }
            entryList
            Toggle("Show hidden", isOn: $showsHidden)
                .toggleStyle(.checkbox)
                .font(.callout)
        }
        .task { await load(startingAt) }
        .onChange(of: showsHidden) {
            Task { await load(listing?.path ?? startingAt) }
        }
    }

    // MARK: Parts

    private var pathBar: some View {
        HStack(spacing: 6) {
            Button("Up", systemImage: "arrow.up") {
                if let parent = listing?.parent { open(path: parent) }
            }
            .labelStyle(.iconOnly)
            .buttonStyle(SquareIconButtonStyle())
            .keyboardShortcut(.upArrow, modifiers: .command)
            .help("Up (⌘↑)")
            .disabled(listing?.parent == nil)
            TextField("Path", text: $typedPath, prompt: Text("/home/me/code"))
                .labelsHidden()
                .textFieldStyle(.plain)
                .font(.system(size: 12.5, design: .monospaced))
                .focused($isPathFocused)
                .onSubmit(go)
                .padding(.horizontal, 10)
                .frame(height: 30)
                .fieldBox(isFocused: isPathFocused)
                .accessibilityLabel("Path on \(model.machineName(.machine(machineID)) ?? "the Machine")")
            Button("Go", action: go)
                .buttonStyle(PillButtonStyle(isCompact: true))
                .disabled(typedPath.trimmingCharacters(in: .whitespaces).isEmpty)
        }
    }

    /// The folders from the root to the one shown; each opens it.
    private var breadcrumb: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 3) {
                ForEach(Array(crumbs.enumerated()), id: \.offset) { index, crumb in
                    if index > 0 {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 8, weight: .semibold))
                            .foregroundStyle(.tertiary)
                            .accessibilityHidden(true)
                    }
                    if index == crumbs.count - 1 {
                        Text(crumb.name)
                            .foregroundStyle(.primary)
                    } else {
                        Button(crumb.name) { open(path: crumb.path) }
                            .buttonStyle(.plain)
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Open \(crumb.path)")
                    }
                }
            }
            .font(.system(size: 11.5, design: .monospaced))
            .lineLimit(1)
        }
        .defaultScrollAnchor(.trailing)
    }

    private var entryList: some View {
        List(selection: $selection) {
            ForEach(error == nil ? entries : [], id: \.name) { entry in
                EntryRow(entry: entry) { open(entry) }
                    .tag(entry.name)
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .focused($isListFocused)
        .contextMenu(forSelectionType: String.self) { _ in
            EmptyView()
        } primaryAction: { names in
            if let entry = names.first.flatMap(entry(named:)) { open(entry) }
        }
        .overlay { listOverlay }
        .frame(height: 260)
        .fieldBox(isFocused: false)
    }

    @ViewBuilder private var listOverlay: some View {
        if let error {
            Label(error, systemImage: "exclamationmark.triangle.fill")
                .font(.callout)
                .foregroundStyle(Color(nsColor: Theme.failed))
                .multilineTextAlignment(.center)
                .padding()
        } else if isLoading && listing == nil {
            ProgressView().controlSize(.small)
        } else if listing != nil && entries.isEmpty {
            Text(mode == .folder ? "No folders here." : "No folders or Project files here.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: State

    /// What the folder holds that can be picked or opened.
    private var entries: [DirectoryEntry] {
        (listing?.entries ?? []).filter { $0.isDirectory || (mode == .projectFile && Self.isProjectFile($0.name)) }
    }

    private func entry(named name: String) -> DirectoryEntry? {
        entries.first { $0.name == name }
    }

    private static func isProjectFile(_ name: String) -> Bool {
        let name = name.lowercased()
        return name.hasSuffix(".yaml") || name.hasSuffix(".yml")
    }

    /// The selected folder, else the one shown; for a Project file, the selected file.
    private var chosenPath: String? {
        guard let listing, error == nil else { return nil }
        let selected = selection.flatMap(entry(named:))
        switch mode {
        case .folder:
            return selected.map { Self.path(of: $0.name, in: listing.path) } ?? listing.path
        case .projectFile:
            return selected.flatMap { $0.isDirectory ? nil : Self.path(of: $0.name, in: listing.path) }
        }
    }

    /// ↩ opens a typed path, or what is selected in the list (a folder opens,
    /// a Project file is chosen), rather than choosing here.
    private var isChooseDefault: Bool {
        if isPathFocused, typedPath != listing?.path { return false }
        if isListFocused, selection != nil { return false }
        return true
    }

    /// The root, then each folder down to the one shown.
    private var crumbs: [(name: String, path: String)] {
        guard let path = listing?.path else { return [] }
        guard path.hasPrefix("/") else { return [(path, path)] }
        var crumbs = [(name: "/", path: "/")]
        var current = ""
        for part in path.split(separator: "/") {
            current += "/\(part)"
            crumbs.append((String(part), current))
        }
        return crumbs
    }

    private static func path(of name: String, in folder: String) -> String {
        folder.hasSuffix("/") ? folder + name : "\(folder)/\(name)"
    }

    // MARK: Actions

    private func go() {
        let path = typedPath.trimmingCharacters(in: .whitespaces)
        guard !path.isEmpty else { return }
        open(path: path)
    }

    private func open(_ entry: DirectoryEntry) {
        guard let listing else { return }
        let path = Self.path(of: entry.name, in: listing.path)
        if entry.isDirectory { open(path: path) } else { pick(path) }
    }

    private func open(path: String) {
        Task { await load(path) }
    }

    private func pick(_ path: String) {
        choose(path)
        dismiss()
    }

    private func load(_ path: String?) async {
        let current = UUID()
        request = current
        isLoading = true
        do {
            let result = try await model.listDirectory(
                path, on: machineID, showHidden: showsHidden, includeFiles: mode == .projectFile
            )
            guard request == current else { return }
            listing = result
            typedPath = result.path
            error = nil
        } catch {
            guard request == current else { return }
            self.error = error.message
            if let path { typedPath = path }
        }
        selection = nil
        isLoading = false
    }
}

/// A folder or Project file in the picker: one element, its name as label
/// and what it is as value.
private struct EntryRow: View {
    let entry: DirectoryEntry
    let open: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: entry.isDirectory ? "folder" : "doc.text")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .frame(width: 16)
            Text(entry.name)
                .font(.system(size: 12.5, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            if entry.isDirectory {
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(entry.name)
        .accessibilityValue(entry.isDirectory ? "Folder" : "Project file")
        .accessibilityAction(named: Text(entry.isDirectory ? "Open" : "Choose"), open)
    }
}
