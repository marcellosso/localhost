import Foundation
import Observation

/// A main-actor mirror of every Service's status, for SwiftUI to observe.
/// It only reflects `ServiceManager`; changes go through the manager.
@MainActor
@Observable
public final class ServiceStatusStore {
    public private(set) var statuses: [UUID: ServiceStatus] = [:]
    @ObservationIgnored private var observation: Task<Void, Never>?

    public init(manager: ServiceManager) {
        observation = Task { [weak self] in
            let (snapshot, events) = manager.subscribe()
            self?.statuses = snapshot
            for await event in events {
                guard let self else { return }
                self.apply(event)
            }
        }
    }

    isolated deinit {
        observation?.cancel()
    }

    public func status(of serviceID: UUID) -> ServiceStatus {
        statuses[serviceID] ?? .neverRun(serviceID)
    }

    private func apply(_ event: ServiceEvent) {
        switch event {
        case .serviceStateChanged(let status):
            statuses[status.serviceID] = status
        case .runStarted(let run), .runEnded(let run):
            if statuses[run.serviceID]?.run?.id == run.id {
                statuses[run.serviceID]?.run = run
            }
        }
    }
}
