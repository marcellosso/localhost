import SwiftUI

struct SettingsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        Form {
            Section {
                Toggle("Start on a free port when a service’s port is in use", isOn: $model.isAutomaticPortMoveEnabled)
            } footer: {
                Text("The service gets the next free port as $PORT for that run; its configured port is kept. Services others depend on, or whose port can’t change, are never moved.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: 460)
        .fixedSize(horizontal: false, vertical: true)
    }
}
