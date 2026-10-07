import Foundation

/// The Dependency graph of a Project's Services. Dependencies are validated
/// when edited, so the graph of a stored Project never has a cycle.
extension Project {
    public func service(id: UUID) -> Service? {
        services.first { $0.id == id }
    }

    /// The Services that depend directly on `serviceID`.
    public func dependents(of serviceID: UUID) -> [Service] {
        services.filter { $0.dependencies.contains(serviceID) }
    }

    /// The cycle that making `serviceID` depend on `dependencyID` would close,
    /// as Keys from `serviceID` back to itself (e.g. `api → worker → api`), or
    /// `nil` if the Dependency is allowed.
    public func dependencyCycle(from serviceID: UUID, to dependencyID: UUID) -> [Key]? {
        guard let service = service(id: serviceID) else { return nil }
        if serviceID == dependencyID { return [service.key, service.key] }
        // Depth-first search for a path dependencyID → … → serviceID.
        var visited: Set<UUID> = []
        func path(from id: UUID) -> [Key]? {
            guard visited.insert(id).inserted, let current = self.service(id: id) else { return nil }
            if id == serviceID { return [current.key] }
            for next in current.dependencies {
                if let rest = path(from: next) { return [current.key] + rest }
            }
            return nil
        }
        return path(from: dependencyID).map { [service.key] + $0 }
    }

    /// `serviceIDs` ordered so that every Service comes after its Dependencies
    /// among them; otherwise in the Project's order.
    public func dependencyOrder(of serviceIDs: [UUID]) -> [UUID] {
        let included = Set(serviceIDs)
        var ordered: [UUID] = []
        var placed: Set<UUID> = []
        func place(_ id: UUID) {
            guard included.contains(id), !placed.contains(id), let service = service(id: id) else { return }
            placed.insert(id)
            service.dependencies.forEach(place)
            ordered.append(id)
        }
        services.map(\.id).filter(included.contains).forEach(place)
        return ordered
    }
}
