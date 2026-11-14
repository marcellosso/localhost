import LocalhostCore
import SwiftUI
import UniformTypeIdentifiers

/// Adds or edits a Service: name, key, working directory relative to the
/// Project, command line, Run before, Python environment, Port and URL,
/// environment variables, Dependencies and whether Start All includes it.
///
/// A new Service takes the virtual environment the core finds for its
/// command until the user edits that field; an existing one only offers it.
struct ServiceForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let project: Project
    let editing: Service?
    var place: AppModel.Place = .thisMac

    @State private var name = ""
    @State private var key = ""
    @State private var isKeyEdited = false
    @State private var workingDirectory = "."
    @State private var commandLine = ""
    @State private var runBefore = ""
    @State private var pythonEnvironment = ""
    @State private var isPythonEnvironmentEdited = false
    @State private var detectedEnvironment: String?
    @State private var port = ""
    @State private var url = ""
    @State private var portCanChange = true
    @State private var variables: [VariableDraft] = []
    @State private var dependencies: [UUID] = []
    @State private var isExcludedFromStartAll = false
    @State private var isChoosingFolder = false
    @State private var error: String?
    @State private var isSaving = false

    var body: some View {
        FormSheet(
            systemImage: "terminal",
            tint: Color(nsColor: serviceColor),
            title: editing == nil ? "New Service" : "Edit \(editing?.name ?? "Service")",
            subtitle: Self.subtitle(of: project, on: model.machineName(place)),
            hint: model.reasonCantChange(place) ?? "↩ to \(editing == nil ? "add" : "save")",
            error: error,
            primaryTitle: editing == nil ? "Add Service" : "Save",
            isPrimaryDisabled: name.isEmpty || key.isEmpty || commandLine.isEmpty || isSaving || !model.canChange(place),
            width: 580,
            cancel: { dismiss() },
            primary: { Task { await save() } }
        ) {
            HStack(alignment: .top, spacing: 12) {
                Field(title: "Name") {
                    BoxedTextField(label: "Name", text: $name, prompt: "Web")
                }
                Field(title: "Key", note: "in logs & CLI") {
                    BoxedTextField(label: "Key", text: keyBinding, prompt: "web", isMonospaced: true)
                }
                .frame(width: 160)
            }
            .onChange(of: name) {
                if !isKeyEdited { key = model.suggestedServiceKey(for: name, in: project, excluding: editing?.id) }
            }
            Field(title: "Command", note: "runs in your login shell") {
                CommandField(workingDirectory: $workingDirectory, commandLine: $commandLine) { isChoosingFolder = true }
            }
            Field(
                title: "Run before",
                note: "optional · same shell, first",
                hint: "Runs before the command, in the same shell, e.g. “nvm use”. If it fails, the service doesn’t start."
            ) {
                BoxedTextField(label: "Run before", text: $runBefore, prompt: "nvm use", isMonospaced: true)
            }
            VStack(alignment: .leading, spacing: 8) {
                Field(title: "Python environment", note: "virtualenv") {
                    PythonEnvironmentField(path: pythonEnvironmentBinding, detected: detectedEnvironment)
                }
                if isUsingDetectedEnvironment, let detectedEnvironment {
                    DetectedEnvironment(path: detectedEnvironment)
                }
                if let pythonEnvironmentHint {
                    FieldHint(pythonEnvironmentHint)
                }
            }
            .task(id: DetectionInput(workingDirectory: workingDirectory, commandLine: commandLine)) {
                try? await Task.sleep(for: .milliseconds(250))
                guard !Task.isCancelled else { return }
                let detected = await model.pythonEnvironment(
                    in: project, workingDirectory: workingDirectory, commandLine: commandLine, on: place
                )
                guard !Task.isCancelled else { return }
                detectedEnvironment = detected
                if !isPythonEnvironmentEdited { pythonEnvironment = detected ?? "" }
            }
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .top, spacing: 12) {
                    Field(title: "Port") {
                        BoxedTextField(label: "Port", text: $port, prompt: "3000", isMonospaced: true)
                    }
                    .frame(width: 120)
                    Field(title: "URL", note: "optional") {
                        BoxedTextField(label: "URL", text: $url,
                                       prompt: port.isEmpty ? "http://localhost:3000" : "http://localhost:\(port)",
                                       isMonospaced: true)
                    }
                }
                FieldHint(portFooter)
                if !port.isEmpty {
                    SwitchRow(title: "Port can change",
                              detail: "If the port is in use, this run may start on the next free one.",
                              isOn: $portCanChange)
                        .padding(.top, 4)
                }
            }
            Field(title: "Environment", note: "overrides the project’s · applies on next start") {
                VariableEditor(variables: $variables, layer: editing.map { .service($0.id) }, place: place)
            }
            if !otherServices.isEmpty {
                Field(title: "Starts after", note: "waits until they’re running", hint: dependencyHint) {
                    FlowLayout {
                        ForEach(otherServices) { other in
                            dependencyChip(other)
                        }
                    }
                }
            }
            SwitchRow(title: "Manual", detail: startFooter, isOn: $isExcludedFromStartAll)
        }
        .fileImporter(isPresented: Binding { isChoosingFolder && place == .thisMac } set: { isChoosingFolder = $0 },
                      allowedContentTypes: [.folder]) { result in
            guard case .success(let url) = result else { return }
            workingDirectory = Self.relativePath(of: url.path, to: project.path)
        }
        .sheet(isPresented: Binding { isChoosingFolder && place != .thisMac } set: { isChoosingFolder = $0 }) {
            if case .machine(let machineID) = place {
                RemoteFolderPicker(machineID: machineID, startingAt: project.path) { folder in
                    workingDirectory = Self.relativePath(of: folder, to: project.path)
                }
            }
        }
        .onAppear {
            guard let editing else { return }
            name = editing.name
            key = editing.key.rawValue
            isKeyEdited = true
            workingDirectory = editing.workingDirectory
            commandLine = editing.commandLine
            runBefore = editing.runBefore ?? ""
            pythonEnvironment = editing.pythonEnvironment ?? ""
            isPythonEnvironmentEdited = true
            port = editing.port.map(String.init) ?? ""
            url = editing.url ?? ""
            portCanChange = editing.portCanChange
            variables = VariableDraft.drafts(from: editing.variables)
            dependencies = editing.dependencies
            isExcludedFromStartAll = editing.isExcludedFromStartAll
        }
    }

    /// What the virtual environment is detected from.
    private struct DetectionInput: Equatable {
        let workingDirectory: String
        let commandLine: String
    }

    private var pythonEnvironmentBinding: Binding<String> {
        Binding { pythonEnvironment } set: { newValue in
            pythonEnvironment = newValue
            isPythonEnvironmentEdited = true
        }
    }

    private var isUsingDetectedEnvironment: Bool {
        let trimmed = pythonEnvironment.trimmingCharacters(in: .whitespaces)
        return !trimmed.isEmpty && trimmed == detectedEnvironment
    }

    private var pythonEnvironmentHint: String? {
        let trimmed = pythonEnvironment.trimmingCharacters(in: .whitespaces)
        if isUsingDetectedEnvironment { return nil }
        if !trimmed.isEmpty {
            return "Its bin folder goes first on PATH and VIRTUAL_ENV is set, as activate does. Relative to the working directory."
        }
        if let detectedEnvironment { return "Off. \(detectedEnvironment) was found next to this Python command." }
        return "Optional. A virtual environment such as .venv, relative to the working directory."
    }

    /// The Service's colour; a new one gets the next colour in the Project.
    private var serviceColor: NSColor {
        let index = editing.flatMap { editing in project.services.firstIndex { $0.id == editing.id } } ?? project.services.count
        return Theme.serviceColor(at: index)
    }

    private var dependencyHint: String? {
        guard let editing, otherServices.contains(where: { project.dependencyCycle(from: editing.id, to: $0.id) != nil }) else { return nil }
        return "Services that already depend on this one can’t be chosen: that would be a cycle."
    }

    private var portFooter: String {
        guard !port.isEmpty else {
            return "Optional. With a port, the service counts as running once the port accepts connections."
        }
        var footer = "The command gets the port as $PORT, e.g. “next dev -p $PORT”. It counts as running once the port accepts connections."
        if variables.contains(where: { $0.name == "PORT" }) {
            footer += " The PORT variable below is replaced by this port."
        }
        if portCanChange, !Service.commandLineUsesPort(commandLine) {
            footer += " The command doesn’t mention $PORT: if it ignores PORT, turn off “Port can change”."
        }
        return footer
    }

    private var otherServices: [Service] {
        project.services.filter { $0.id != editing?.id }
    }

    private func dependencyChip(_ other: Service) -> some View {
        // Only an existing Service can have dependents, so only it can close a cycle.
        let cycle = editing.flatMap { project.dependencyCycle(from: $0.id, to: other.id) }
        let isOn = Binding {
            dependencies.contains(other.id)
        } set: { isOn in
            if isOn { dependencies.append(other.id) } else { dependencies.removeAll { $0 == other.id } }
        }
        let index = project.services.firstIndex { $0.id == other.id } ?? 0
        return ChipToggle(title: other.name, color: Color(nsColor: Theme.serviceColor(at: index)), isOn: isOn)
            .disabled(cycle != nil)
            .opacity(cycle != nil ? 0.4 : 1)
            .help(cycle.map { "\($0.map(\.rawValue).joined(separator: " → ")) would be a dependency cycle." } ?? other.key.rawValue)
    }

    private var startFooter: String {
        var footer = "Start All skips it; start it on its own."
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
        let inputs = variables.compactMap(\.input)
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
                    commandLine: commandLine, variables: inputs, dependencies: dependencies,
                    isExcludedFromStartAll: isExcludedFromStartAll, port: parsedPort, url: url,
                    portCanChange: portCanChange, pythonEnvironment: pythonEnvironment, runBefore: runBefore, on: place
                )
            } else {
                try await model.addService(
                    to: project.id, name: name, key: key, workingDirectory: workingDirectory,
                    commandLine: commandLine, variables: inputs, dependencies: dependencies,
                    isExcludedFromStartAll: isExcludedFromStartAll, port: parsedPort, url: url,
                    portCanChange: portCanChange, pythonEnvironment: pythonEnvironment, runBefore: runBefore, on: place
                )
            }
            dismiss()
        } catch {
            self.error = error.message
        }
    }

    /// "in **My SaaS** · ~/code/my-saas"; on another Machine, "in **My SaaS**
    /// on devbox · /srv/my-saas", its path as it is there.
    static func subtitle(of project: Project, on machine: String?) -> Text {
        guard let machine else {
            return Text("in **\(project.name)** · ") + Text((project.path as NSString).abbreviatingWithTildeInPath).monospaced()
        }
        return Text("in **\(project.name)** on \(machine) · ") + Text(project.path).monospaced()
    }

    /// `path` relative to `base`, e.g. "web" or "../shared"; "." for `base`
    /// itself. Also for another Machine's paths: both sides are standardized alike.
    static func relativePath(of path: String, to base: String) -> String {
        let target = URL(filePath: path).standardizedFileURL.pathComponents
        let root = URL(filePath: base).standardizedFileURL.pathComponents
        let common = zip(target, root).prefix { $0 == $1 }.count
        let components = Array(repeating: "..", count: root.count - common) + target.dropFirst(common)
        return components.isEmpty ? "." : components.joined(separator: "/")
    }
}
