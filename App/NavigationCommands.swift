import LocalhostCore
import SwiftUI

/// ⌘1–9 select Projects, ⌥⌘1–9 select the selected Project's Services,
/// ⌘[ / ⌘] step through its Services. (⌘0 Ports lives in the Window menu.)
struct NavigationCommands: Commands {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandMenu("Go") {
            ForEach(Array(model.projects.prefix(9).enumerated()), id: \.element.id) { index, project in
                Button(project.name) {
                    openWindow(id: "main")
                    model.selectedProjectID = project.id
                }
                .keyboardShortcut(KeyEquivalent(Character(String(index + 1))))
            }
            let services = model.selectedProject?.services ?? []
            if !services.isEmpty {
                Divider()
                ForEach(Array(services.prefix(9).enumerated()), id: \.element.id) { index, service in
                    Button(service.name) {
                        openWindow(id: "main")
                        model.selectedServiceID = service.id
                    }
                    .keyboardShortcut(KeyEquivalent(Character(String(index + 1))), modifiers: [.command, .option])
                }
            }
            Divider()
            Button("Previous Service") { model.selectAdjacentService(by: -1) }
                .keyboardShortcut("[")
                .disabled(services.isEmpty)
            Button("Next Service") { model.selectAdjacentService(by: 1) }
                .keyboardShortcut("]")
                .disabled(services.isEmpty)
        }
    }
}

extension AppModel {
    /// Selects the Service `offset` rows away in the selected Project, stopping
    /// at either end. With none selected, steps in from the first or last.
    func selectAdjacentService(by offset: Int) {
        guard let services = selectedProject?.services, !services.isEmpty else { return }
        guard let current = services.firstIndex(where: { $0.id == selectedServiceID }) else {
            selectedServiceID = (offset > 0 ? services.first : services.last)?.id
            return
        }
        selectedServiceID = services[min(max(current + offset, 0), services.count - 1)].id
    }
}
