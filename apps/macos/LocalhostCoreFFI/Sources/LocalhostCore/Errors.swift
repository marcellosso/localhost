import Foundation

extension LocalhostError {
    /// What went wrong, written for the user.
    public var message: String { localhostErrorMessage(error: self) }
}

extension ProjectFileProblem {
    public var message: String { projectFileProblemMessage(problem: self) }
}
