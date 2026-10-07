import LocalhostCore
import SwiftUI

struct ContentView: View {
    var body: some View {
        Text("localhost \(LocalhostCore.version)")
            .font(.system(.body, design: .monospaced))
            .frame(minWidth: 480, minHeight: 320)
    }
}

#Preview {
    ContentView()
}
