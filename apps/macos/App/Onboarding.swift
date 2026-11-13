import Foundation
import LocalhostCore

/// Where the user is in the first run: a short welcome, then a Get started
/// checklist until the first Service has run. Both are for people new to
/// localhost; someone who opened it before, or already has Projects, sees
/// neither. Kept in UserDefaults, as app state. The words are the desktop
/// app's too (`apps/desktop/src/onboarding.ts`): change both together.
struct Onboarding: Equatable {
    /// The welcome was finished or skipped.
    var hasSeenWelcome = false
    /// The checklist is over: dismissed, complete, or never shown to someone
    /// who used localhost before.
    var isChecklistDone = false
    /// A Service has been Running, which keeps its row ticked.
    var hasStartedService = false

    // MARK: Words

    struct WelcomeStep {
        let title: String
        /// Markdown: commands in backticks.
        let body: String
    }

    static let welcomeTitle = "Welcome to localhost"

    static let welcomeSteps = [
        WelcomeStep(
            title: "A project is a folder",
            body: "Add a project folder once. localhost remembers how it runs, so you start it from here instead of a row of terminal tabs."
        ),
        WelcomeStep(
            title: "A service is a command that keeps running",
            body: "Like `pnpm dev` or `docker compose up`. When you add a folder, localhost suggests its services for you to review."
        ),
        WelcomeStep(
            title: "The port tells localhost it’s up",
            body: "Give a service the port it listens on. Once the port answers, the service is Running, and Open in Browser goes straight to it."
        ),
    ]

    /// The empty states' one line about what they ask for.
    static let projectConcept = "A project is a folder on this computer. Add one and localhost suggests the services that run it."
    static let serviceConcept = "A service is a command that keeps running in this project, like “pnpm dev” or “docker compose up”."

    static let checklistTitle = "Get started"

    enum ChecklistStep {
        case project, service, start, agents
    }

    struct ChecklistItem: Identifiable {
        let step: ChecklistStep
        let title: String
        let isDone: Bool
        let isOptional: Bool

        var id: ChecklistStep { step }
    }

    // MARK: Rules

    /// Where the first run stands at the first launch that has it: nothing
    /// to show for someone who opened localhost before or already has
    /// Projects. After that, what was saved.
    static func atLaunch(saved: Onboarding?, hasOpenedBefore: Bool, projectCount: Int) -> Onboarding {
        if let saved { return saved }
        let isReturning = hasOpenedBefore || projectCount > 0
        return Onboarding(hasSeenWelcome: isReturning, isChecklistDone: isReturning)
    }

    /// The welcome opens only before the first Project: one added another way makes it moot.
    func isWelcomeDue(projectCount: Int) -> Bool {
        !hasSeenWelcome && projectCount == 0
    }

    /// Shown once there is a Project, until dismissed or complete.
    func isChecklistShown(projectCount: Int) -> Bool {
        !isChecklistDone && projectCount > 0
    }

    /// Each row, ticked from what is true now; "Start" stays ticked once a Service has been Running.
    func checklistItems(projects: [Project], hasRunningService: Bool, hasConnectedAgent: Bool) -> [ChecklistItem] {
        [
            ChecklistItem(step: .project, title: "Add a project", isDone: !projects.isEmpty, isOptional: false),
            ChecklistItem(
                step: .service, title: "Add a service",
                isDone: projects.contains { !$0.services.isEmpty }, isOptional: false
            ),
            ChecklistItem(
                step: .start, title: "Start your first service",
                isDone: hasStartedService || hasRunningService, isOptional: false
            ),
            ChecklistItem(step: .agents, title: "Connect your agents", isDone: hasConnectedAgent, isOptional: true),
        ]
    }

    // MARK: Saving

    private static let welcomeKey = "onboardingHasSeenWelcome"
    private static let checklistKey = "onboardingChecklistDone"
    private static let startedKey = "onboardingHasStartedService"

    /// What was saved; `nil` before the first launch that has the welcome.
    static func load(from defaults: UserDefaults = .standard) -> Onboarding? {
        guard defaults.object(forKey: welcomeKey) != nil else { return nil }
        return Onboarding(
            hasSeenWelcome: defaults.bool(forKey: welcomeKey),
            isChecklistDone: defaults.bool(forKey: checklistKey),
            hasStartedService: defaults.bool(forKey: startedKey)
        )
    }

    func save(to defaults: UserDefaults = .standard) {
        defaults.set(hasSeenWelcome, forKey: Self.welcomeKey)
        defaults.set(isChecklistDone, forKey: Self.checklistKey)
        defaults.set(hasStartedService, forKey: Self.startedKey)
    }
}

extension AgentStatus {
    /// Its configuration has localhost's server, even an older entry.
    var hasLocalhost: Bool {
        connection == .connected || connection == .needsUpdate
    }
}
