import LocalhostCore
import SwiftUI
import UniformTypeIdentifiers

/// Adds or edits a Service: name, key, working directory relative to the
/// Project, command line, Port and URL, environment variables, Dependencies
/// and whether Start All includes it.
struct ServiceForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let project: Project
    let editing: Service?

    @State private var name = ""
    @State private var key = ""
    @State private var isKeyEdited = false
    @State private var workingDirectory = "."
    @State private var commandLine = ""
    @State private var port = ""
    @State private var url = ""
    @State private var variables: [Variable] = []
    @State private var dependencies: [UUID] = []
    @State private var isExcludedFromStartAll = false
    @State private var isChoosingFolder = false
    @State private var error: String?
    @State private var isSaving = false

    struct Variable: Identifiable {
        let id = UUID()
        var name = ""
        var value = ""
    }

    var body: some View {
        Form {
            Section {
                TextField("Name", text: $name, prompt: Text("Web"))
                    .onChange(of: name) {
                        if !isKeyEdited { key = model.suggestedServiceKey(for: name, in: project, excluding: editing?.id) }
                    }
                TextField("Key", text: keyBinding, prompt: Text("web"))
                    .font(.body.monospaced())
            }
            Section {
                LabeledContent("Working Directory") {
                    HStack {
                        TextField("Working Directory", text: $workingDirectory, prompt: Text("."))
                            .labelsHidden()
                            .font(.body.monospaced())
                        Button("Choose…") { isChoosingFolder = true }
                    }
                }
                TextField("Command Line", text: $commandLine, prompt: Text("pnpm dev"))
                    .font(.body.monospaced())
            } footer: {
                Text("Relative to \(project.path). The command line runs in your login shell.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Section {
                TextField("Port", text: $port, prompt: Text("3000"))
                    .font(.body.monospaced())
                TextField("URL", text: $url, prompt: Text(port.isEmpty ? "http://localhost:3000" : "http://localhost:\(port)"))
                    .font(.body.monospaced())
            } footer: {
                Text(portFooter)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Section("Environment Variables") {
                ForEach($variables) { $variable in
                    HStack {
                        TextField("Name", text: $variable.name, prompt: Text("NAME"))
                            .labelsHidden()
                            .font(.body.monospaced())
                        TextField("Value", text: $variable.value, prompt: Text("value"))
                            .labelsHidden()
                            .font(.body.monospaced())
                        Button("Remove Variable", systemImage: "minus.circle") {
                            variables.removeAll { $0.id == variable.id }
                        }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                    }
                }
                Button("Add Variable", systemImage: "plus") { variables.append(Variable()) }
                    .buttonStyle(.borderless)
            }
            startSection
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
        }
        .formStyle(.grouped)
        .frame(width: 520)
        .navigationTitle(editing == nil ? "Add Service" : "Edit Service")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(editing == nil ? "Add" : "Save") { Task { await save() } }
                    .disabled(name.isEmpty || key.isEmpty || commandLine.isEmpty || isSaving)
            }
        }
        .fileImporter(isPresented: $isChoosingFolder, allowedContentTypes: [.folder]) { result in
            guard case .success(let url) = result else { return }
            workingDirectory = Self.relativePath(of: url.path, to: project.path)
        }
        .onAppear {
            guard let editing else { return }
            name = editing.name
            key = editing.key.rawValue
            isKeyEdited = true
            workingDirectory = editing.workingDirectory
            commandLine = editing.commandLine
            port = editing.port.map(String.init) ?? ""
            url = editing.url ?? ""
            variables = editing.environment.sorted { $0.key < $1.key }.map { Variable(name: $0.key, value: $0.value) }
            dependencies = editing.dependencies
            isExcludedFromStartAll = editing.isExcludedFromStartAll
        }
    }

    private var portFooter: String {
        guard !port.isEmpty else {
            return "Optional. With a port, the service counts as running once the port accepts connections."
        }
        var footer = "The command gets the port as $PORT, e.g. “next dev -p $PORT”. The service counts as running once the port accepts connections."
        if variables.contains(where: { $0.name == "PORT" }) {
            footer += " The PORT variable below is replaced by this port."
        }
        return footer
    }

    private var otherServices: [Service] {
        project.services.filter { $0.id != editing?.id }
    }

    @ViewBuilder
    private var startSection: some View {
        Section {
            if !otherServices.isEmpty {
                LabeledContent("Depends On") {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(otherServices) { other in
                            dependencyToggle(other)
                        }
                    }
                }
            }
            Toggle("Not in Start All", isOn: $isExcludedFromStartAll)
        } header: {
            Text("Starting")
        } footer: {
            Text(startFooter)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func dependencyToggle(_ other: Service) -> some View {
        // Only an existing Service can have dependents, so only it can close a cycle.
        let cycle = editing.flatMap { project.dependencyCycle(from: $0.id, to: other.id) }
        let isOn = Binding {
            dependencies.contains(other.id)
        } set: { isOn in
            if isOn { dependencies.append(other.id) } else { dependencies.removeAll { $0 == other.id } }
        }
        return Toggle(isOn: isOn) {
            HStack(spacing: 6) {
                Text(other.name)
                Text(other.key.rawValue)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
            }
        }
        .disabled(cycle != nil)
        .help(cycle.map { "\($0.map(\.rawValue).joined(separator: " → ")) would be a dependency cycle." } ?? "")
    }

    private var startFooter: String {
        var footer = "Starting this service starts the services it depends on first, and waits until they are running or have exited."
        let dependents = editing.map { project.dependents(of: $0.id).map { "“\($0.name)”" } } ?? []
        if isExcludedFromStartAll {
            footer = dependents.isEmpty
                ? "Start All skips this service."
                : "Start All skips this service, but still starts it for \(ListFormatter.localizedString(byJoining: dependents)), which depend\(dependents.count == 1 ? "s" : "") on it."
        }
        return footer
    }

    private var keyBinding: Binding<String> {
        Binding { key } set: { newValue in
            key = newValue
            isKeyEdited = true
        }
    }

    private func save() async {
        let filled = variables.filter { !$0.name.isEmpty || !$0.value.isEmpty }
        let names = filled.map(\.name)
        if let duplicate = names.first(where: { name in names.filter { $0 == name }.count > 1 }) {
            error = "“\(duplicate)” is defined more than once."
            return
        }
        let environment = Dictionary(uniqueKeysWithValues: filled.map { ($0.name, $0.value) })
        let trimmedPort = port.trimmingCharacters(in: .whitespaces)
        let parsedPort = Int(trimmedPort)
        if !trimmedPort.isEmpty, parsedPort == nil {
            error = "The port must be a number, e.g. 3000."
            return
        }

        isSaving = true
        defer { isSaving = false }
        do {
            if let editing {
                try await model.editService(
                    editing.id, name: name, key: key, workingDirectory: workingDirectory,
                    commandLine: commandLine, environment: environment, dependencies: dependencies,
                    isExcludedFromStartAll: isExcludedFromStartAll, port: parsedPort, url: url
                )
            } else {
                try await model.addService(
                    to: project.id, name: name, key: key, workingDirectory: workingDirectory,
                    commandLine: commandLine, environment: environment, dependencies: dependencies,
                    isExcludedFromStartAll: isExcludedFromStartAll, port: parsedPort, url: url
                )
            }
            dismiss()
        } catch {
            self.error = error.message
        }
    }

    /// `path` relative to `base`, e.g. "web" or "../shared"; "." for `base` itself.
    static func relativePath(of path: String, to base: String) -> String {
        let target = URL(filePath: path).standardizedFileURL.pathComponents
        let root = URL(filePath: base).standardizedFileURL.pathComponents
        let common = zip(target, root).prefix { $0 == $1 }.count
        let components = Array(repeating: "..", count: root.count - common) + target.dropFirst(common)
        return components.isEmpty ? "." : components.joined(separator: "/")
    }
}
