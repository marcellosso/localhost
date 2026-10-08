import AppKit
import SwiftUI

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage(AppModel.editorApplicationKey) private var editorPath: String?

    var body: some View {
        @Bindable var model = model
        Form {
            Section {
                LabeledContent("Editor") {
                    HStack(spacing: 6) {
                        if let editorPath {
                            Image(nsImage: NSWorkspace.shared.icon(forFile: editorPath))
                                .resizable()
                                .frame(width: 16, height: 16)
                            Text(FileManager.default.displayName(atPath: editorPath))
                        } else {
                            Text("None").foregroundStyle(.secondary)
                        }
                        Button("Choose…") { AppModel.chooseEditor() }
                    }
                }
            } footer: {
                Text("“Open in Editor” (⌥⌘E) opens projects in this app.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
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
