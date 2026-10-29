import Foundation
import LocalhostCore

// Started by this app or `localhost mcp` as the shared core of a storage
// root (`Targets`): serve it, with no window, until no client needs it.
let arguments = CommandLine.arguments
if arguments.count == 3, arguments[1] == "--daemon" {
    if let problem = runSharedCore(storageRoot: URL(filePath: arguments[2], directoryHint: .isDirectory)) {
        FileHandle.standardError.write(Data("localhost: \(problem)\n".utf8))
        exit(1)
    }
    exit(0)
}
LocalhostApp.main()
