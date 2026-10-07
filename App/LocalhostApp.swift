import LocalhostCore
import SwiftUI

@main
struct LocalhostApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(model)
                .task { await model.checkForOrphans() }
        }
    }
}
