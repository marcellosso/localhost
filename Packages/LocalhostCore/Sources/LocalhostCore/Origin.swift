/// Who or what requested a state-changing action.
///
/// Every mutation of the core takes an Origin so clients (and later agents)
/// can tell who changed what.
public enum Origin: String, Sendable, Codable, Hashable {
    case mainWindow
    case menuBar
}
