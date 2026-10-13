import AppKit
import LocalhostCore

/// One thing the command palette (⌘K) can do: jump to a Project or Service,
/// or run an action on one.
struct PaletteItem: Identifiable {
    enum Icon {
        case symbol(String)
        /// A Service: its colour and state glyph.
        case service(color: NSColor, state: ServiceState)
    }

    /// Stable across rebuilds, e.g. `start:<service id>`, so the selection survives typing.
    let id: String
    let title: String
    var subtitle: String?
    let icon: Icon
    /// The shortcut that does the same outside the palette, e.g. "⌘R".
    var shortcut: String?
    /// Extra words it matches, e.g. the Service's Key and its Project's name.
    var keywords: String = ""
    /// Ranks it above equally good matches (jumps over actions, the selection over the rest).
    var boost = 0
    let action: () -> Void
}

extension AppModel {
    /// Everything the palette offers for the current state. An empty query
    /// shows the first ones: the selected Service's actions, the selected
    /// Project's Services and actions, the other Projects, Ports and New
    /// Project. The rest (ids starting `other:`) are reachable by typing.
    func paletteItems() -> [PaletteItem] {
        var items: [PaletteItem] = []
        let selected = selectedProject
        if let service = selectedService, let selected {
            items += serviceActions(service, in: selected, isSelected: true)
        }
        if let selected {
            items += selected.services.enumerated().map { serviceJump($1, in: selected, index: $0, isInSelectedProject: true) }
            items += projectActions(selected, isSelected: true)
        }
        for (index, project) in projects.enumerated() where project.id != selected?.id {
            items.append(projectJump(project, index: index))
        }
        items.append(PaletteItem(id: "ports", title: "Ports", subtitle: "Every listening port on this Mac",
                                 icon: .symbol("network"), shortcut: "⌘0", keywords: "listeners") { [weak self] in
            self?.sidebarSelection = .ports
        })
        if core != nil {
            items.append(PaletteItem(id: "new-project", title: "New Project…", icon: .symbol("folder.badge.plus"),
                                     shortcut: "⌘N", keywords: "add") { [weak self] in
                    self?.presentedSheet = .addProject
            })
        }
        // Only reachable by typing: other Projects' actions and Services.
        for project in projects where project.id != selected?.id {
            items += project.services.enumerated().map { serviceJump($1, in: project, index: $0, isInSelectedProject: false) }
            items += projectActions(project, isSelected: false)
            for service in project.services {
                items += serviceActions(service, in: project, isSelected: false)
            }
        }
        if let selected {
            for service in selected.services where service.id != selectedServiceID {
                items += serviceActions(service, in: selected, isSelected: false)
            }
        }
        return items
    }

    /// How many items an empty query shows: the ones about the selection and the jumps.
    static let paletteEmptyQueryLimit = 30

    /// The items matching `query`, best first.
    func paletteResults(_ query: String, in items: [PaletteItem]) -> [PaletteItem] {
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else {
            return Array(items.prefix { !$0.id.hasPrefix("other:") }.prefix(Self.paletteEmptyQueryLimit))
        }
        return items.enumerated()
            .compactMap { offset, item -> (PaletteItem, Int, Int)? in
                guard let score = FuzzyMatch.score(query, in: "\(item.title) \(item.subtitle ?? "") \(item.keywords)") else { return nil }
                return (item, score + item.boost, offset)
            }
            .sorted { ($0.1, -$0.2) > ($1.1, -$1.2) }
            .prefix(50)
            .map(\.0)
    }

    // MARK: Items

    private func projectJump(_ project: Project, index: Int) -> PaletteItem {
        PaletteItem(
            id: "project:\(project.id)", title: project.name,
            subtitle: (project.path as NSString).abbreviatingWithTildeInPath,
            icon: .symbol("folder"), shortcut: index < 9 ? "⌘\(index + 1)" : nil,
            keywords: "\(project.key) go project", boost: 3
        ) { [weak self] in
            self?.selectedProjectID = project.id
        }
    }

    private func serviceJump(
        _ service: Service, in project: Project, index: Int, isInSelectedProject: Bool
    ) -> PaletteItem {
        PaletteItem(
            id: "\(isInSelectedProject ? "" : "other:")service:\(service.id)", title: service.name,
            subtitle: "\(project.name) · \(service.commandLine)",
            icon: .service(color: Theme.serviceColor(at: index), state: status(of: service).state),
            shortcut: isInSelectedProject && index < 9 ? "⌥⌘\(index + 1)" : nil,
            keywords: "\(service.key) \(project.key) go service", boost: isInSelectedProject ? 4 : 2
        ) { [weak self] in
            self?.selectedProjectID = project.id
            self?.selectedServiceID = service.id
        }
    }

    private func serviceActions(_ service: Service, in project: Project, isSelected: Bool) -> [PaletteItem] {
        let prefix = isSelected ? "" : "other:"
        let subtitle = project.name
        let keywords = "\(service.key) \(project.name) \(project.key)"
        let boost = isSelected ? 2 : 0
        let state = status(of: service).state
        var items: [PaletteItem] = []
        func item(_ action: String, _ title: String, _ symbol: String, _ shortcut: String?, perform: @escaping () -> Void) {
            items.append(PaletteItem(id: "\(prefix)\(action):\(service.id)", title: title, subtitle: subtitle,
                                     icon: .symbol(symbol), shortcut: isSelected ? shortcut : nil,
                                     keywords: keywords, boost: boost, action: perform))
        }
        switch state {
        case .starting, .running:
            item("restart", "Restart \(service.name)", "arrow.clockwise", "⌘R") { [weak self] in self?.restart(service.id) }
            item("stop", "Stop \(service.name)", "stop.fill", "⌘.") { [weak self] in self?.stop(service.id) }
            if urlToOpen(service) != nil {
                item("open-url", "Open \(service.name) in Browser", "safari", "⇧⌘O") { [weak self] in self?.openURL(of: service.id) }
            }
        case .stopping:
            break
        case .stopped, .exited, .failed:
            item("start", "Start \(service.name)", "play.fill", "⌘R") { [weak self] in self?.start(service.id) }
        }
        let folder = project.workingDirectoryURL(for: service)
        item("terminal", "Open \(service.name) in Terminal", "terminal", nil) { [weak self] in self?.openTerminal(at: folder) }
        item("editor", "Open \(service.name) in Editor", "chevron.left.forwardslash.chevron.right", nil) { [weak self] in
            self?.openInEditor(folder)
        }
        return items
    }

    private func projectActions(_ project: Project, isSelected: Bool) -> [PaletteItem] {
        let prefix = isSelected ? "" : "other:"
        let folder = Self.folder(of: project)
        let keywords = project.key.rawValue
        let boost = isSelected ? 1 : 0
        var items: [PaletteItem] = []
        func item(_ action: String, _ title: String, _ symbol: String, _ shortcut: String?, perform: @escaping () -> Void) {
            items.append(PaletteItem(id: "\(prefix)\(action):\(project.id)", title: title, subtitle: nil,
                                     icon: .symbol(symbol), shortcut: isSelected ? shortcut : nil,
                                     keywords: keywords, boost: boost, action: perform))
        }
        if !project.services.isEmpty {
            item("start-all", "Start All in \(project.name)", "play.fill", "⇧⌘R") { [weak self] in self?.startAll(inProject: project.id) }
        }
        if project.services.contains(where: { status(of: $0).state.isActive }) {
            item("stop-all", "Stop All in \(project.name)", "stop.fill", "⇧⌘.") { [weak self] in self?.stopAll(inProject: project.id) }
        }
        item("project-editor", "Open \(project.name) in Editor", "chevron.left.forwardslash.chevron.right", "⌥⌘E") { [weak self] in
            self?.openInEditor(folder)
        }
        item("project-terminal", "Open \(project.name) in Terminal", "terminal", nil) { [weak self] in self?.openTerminal(at: folder) }
        item("project-finder", "Reveal \(project.name) in Finder", "folder", "⌥⌘F") { [weak self] in self?.revealInFinder(folder) }
        return items
    }
}
