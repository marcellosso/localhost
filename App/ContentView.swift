import LocalhostCore
import SwiftUI

struct ContentView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        VStack(spacing: 8) {
            Text("localhost \(LocalhostCore.version)")
            if let error = model.startupError {
                Text("Could not load projects: \(String(describing: error))")
                    .foregroundStyle(.red)
            }
            if model.isStoppingOrphans {
                ProgressView("Stopping services from the last session…")
                    .controlSize(.small)
            }
        }
        .font(.system(.body, design: .monospaced))
        .frame(minWidth: 480, minHeight: 320)
        .alert(model.orphanAlertTitle, isPresented: $model.isShowingOrphanAlert) {
            Button("Stop Them") {
                Task { await model.stopOrphans() }
            }
            .keyboardShortcut(.defaultAction)
            Button("Leave Them", role: .cancel) {
                model.leaveOrphans()
            }
        } message: {
            Text(model.orphanDescriptions.joined(separator: "\n"))
        }
    }
}

#Preview {
    ContentView()
        .environment(AppModel())
}
