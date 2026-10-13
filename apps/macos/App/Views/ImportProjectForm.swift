import LocalhostCore
import SwiftUI
import UniformTypeIdentifiers

/// The review step of Import Project…: where the Project's folder is on this
/// Mac, what to do if its Key is taken, and what the file brings (Services,
/// environment, and the Secrets that still need values). The file was
/// validated when it was read; nothing is saved until Import or Replace.
struct ImportProjectForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let document: ProjectDocument
    let fileName: String

    @State private var path = ""
    @State private var choice: ProjectLibrary.KeyClash = .keepBoth
    @State private var freeKey: Key?
    @State private var isChoosingFolder = false
    @State private var error: String?
    @State private var isSaving = false

    var body: some View {
        let clash = model.projectClashing(with: document)
        let isReplacing = clash != nil && choice == .replace
        FormSheet(
            systemImage: "square.and.arrow.down",
            title: "Import \(document.name)",
            subtitle: Text("from ") + Text(fileName).monospaced(),
            hint: !folderExists ? nil : isReplacing ? "Replaces “\(clash?.name ?? "")”" : "↩ to import",
            error: error,
            primaryTitle: isReplacing ? "Replace" : "Import",
            isPrimaryDisabled: !folderExists || isSaving,
            width: 600,
            cancel: { dismiss() },
            primary: { Task { await save(onKeyClash: clash == nil ? nil : choice) } }
        ) {
            Field(title: "Folder", hint: folderExists ? nil : "This folder isn’t on this Mac. Choose where the project is.") {
                FolderButton(path: path) { isChoosingFolder = true }
            }
            if let clash {
                Field(title: "Key", note: "“\(document.key)” is already used by \(clash.name)",
                      hint: isReplacing ? replacementHint(clash) : nil) {
                    Picker("Key “\(document.key)” is already used by \(clash.name)", selection: $choice) {
                        Text("Keep both: import as \(freeKey?.rawValue ?? "a new key")")
                            .tag(ProjectLibrary.KeyClash.keepBoth)
                        Text("Replace “\(clash.name)”: keep its logs, use this file’s configuration")
                            .tag(ProjectLibrary.KeyClash.replace)
                    }
                    .pickerStyle(.radioGroup)
                    .labelsHidden()
                }
            }
            Field(title: "Services", note: document.services.isEmpty ? nil : "\(document.services.count)") {
                if document.services.isEmpty {
                    FieldHint("The file has no services.")
                } else {
                    FormCard {
                        ForEach(Array(document.services.enumerated()), id: \.element.key) { index, service in
                            if index > 0 { Divider().padding(.leading, 29) }
                            ImportedServiceRow(service: service, color: Theme.serviceColor(at: index))
                        }
                    }
                }
            }
            if let environment = environmentSummary {
                Field(title: "Environment") {
                    FieldHint(environment)
                }
            }
            if !secrets.isEmpty {
                Field(title: "Secrets to set", note: "\(secrets.count)",
                      hint: "Secret values aren’t in the file. Set them after importing"
                          + (isReplacing ? "; values already stored under the same name for the same service or profile are kept." : ".")) {
                    FormCard {
                        ForEach(Array(secrets.enumerated()), id: \.offset) { index, secret in
                            if index > 0 { Divider().padding(.leading, 12) }
                            HStack(spacing: 8) {
                                Image(systemName: "lock.fill")
                                    .font(.system(size: 10))
                                    .foregroundStyle(.secondary)
                                Text(secret.name)
                                    .font(.system(size: 12, design: .monospaced))
                                Spacer(minLength: 8)
                                Text(secret.owner)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 7)
                        }
                    }
                }
            }
        }
        .fileImporter(isPresented: $isChoosingFolder, allowedContentTypes: [.folder]) { result in
            if case .success(let url) = result { path = url.path }
        }
        .onAppear { path = document.path }
        .task { freeKey = await model.freeProjectKey(basedOn: document.key) }
    }

    private var folderExists: Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: (path as NSString).expandingTildeInPath, isDirectory: &isDirectory)
            && isDirectory.boolValue
    }

    /// What Replace removes from the existing Project, e.g. "Removes Web (running, stopped first)."
    private func replacementHint(_ existing: Project) -> String? {
        let keptKeys = Set(document.services.map(\.key))
        let removed = existing.services.filter { !keptKeys.contains($0.key) }
        guard !removed.isEmpty else { return nil }
        let names = removed.map { model.status(of: $0).state.isActive ? "\($0.name) (running, stopped first)" : $0.name }
        return "Removes " + names.joined(separator: ", ") + ", which the file doesn’t have."
    }

    /// e.g. "2 variables · profiles Development (active), Testing".
    private var environmentSummary: String? {
        var parts: [String] = []
        let count = document.variables.values.count
        if count > 0 { parts.append(count == 1 ? "1 project variable" : "\(count) project variables") }
        if !document.profiles.isEmpty {
            let names = document.profiles.map { $0.name == document.activeProfile ? "\($0.name) (active)" : $0.name }
            parts.append((document.profiles.count == 1 ? "profile " : "profiles ") + names.joined(separator: ", "))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// Every Secret the file names, with where it belongs.
    private var secrets: [(name: String, owner: String)] {
        document.variables.secretNames.map { ($0, "project") }
            + document.services.flatMap { service in service.variables.secretNames.map { ($0, service.name) } }
            + document.profiles.flatMap { profile in profile.variables.secretNames.map { ($0, "profile \(profile.name)") } }
    }

    private func save(onKeyClash: ProjectLibrary.KeyClash?) async {
        let document = document.withPath(path)
        isSaving = true
        defer { isSaving = false }
        do {
            try await model.importProject(document, onKeyClash: onKeyClash)
            dismiss()
        } catch {
            self.error = error.message
        }
    }
}

/// One Service of the file: its colour, name, Key, `./dir $ command`, Port
/// and what it depends on.
private struct ImportedServiceRow: View {
    let service: ProjectDocument.Service
    let color: NSColor

    var body: some View {
        HStack(spacing: 10) {
            RoundedRectangle(cornerRadius: 2)
                .fill(Color(nsColor: color))
                .frame(width: 7, height: 7)
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(service.name)
                        .font(.system(size: 12.5, weight: .semibold))
                        .lineLimit(1)
                    Text(service.key.rawValue)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    if service.isExcludedFromStartAll {
                        Text("not in Start All")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                }
                Text("\(service.workingDirectory == "." ? "./" : "./\(service.workingDirectory)") $ \(service.commandLine)")
                    .font(.system(size: 11.5, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if !service.dependencies.isEmpty {
                    Text("after " + service.dependencies.map(\.rawValue).joined(separator: ", "))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 8)
            if let port = service.port {
                PortPill(port: port, color: color)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .accessibilityElement(children: .combine)
    }
}
