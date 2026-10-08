import Foundation
import Security

/// The variables of one layer of a Run's environment (a Project, a Service,
/// or an Environment Profile): plain values, plus the names of its Secrets,
/// whose values never leave the `SecretStore`.
public struct Variables: Hashable, Sendable, Codable {
    public internal(set) var values: [String: String]
    /// Sorted.
    public internal(set) var secretNames: [String]

    public static let empty = Variables(values: [:], secretNames: [])

    public var isEmpty: Bool { values.isEmpty && secretNames.isEmpty }

    /// Every variable name, plain and Secret, sorted.
    public var names: [String] { (Array(values.keys) + secretNames).sorted() }
}

/// One variable as set from an editor. A Secret's value is written to the
/// `SecretStore`; it is never kept in the Project Configuration.
public struct VariableInput: Sendable, Hashable {
    public enum Value: Sendable, Hashable {
        case plain(String)
        case secret(String)
        /// A Secret that keeps its stored value, possibly under a new name.
        case existingSecret(previousName: String)
    }

    public let name: String
    public let value: Value

    public init(name: String, value: Value) {
        self.name = name
        self.value = value
    }

    public static func plain(_ name: String, _ value: String) -> VariableInput {
        VariableInput(name: name, value: .plain(value))
    }

    public static func secret(_ name: String, _ value: String) -> VariableInput {
        VariableInput(name: name, value: .secret(value))
    }

    /// The layer's Secret `previousName` (by default, `name`), unchanged.
    public static func existingSecret(_ name: String, previousName: String? = nil) -> VariableInput {
        VariableInput(name: name, value: .existingSecret(previousName: previousName ?? name))
    }

    /// Plain variables, sorted by name.
    public static func plain(_ values: [String: String]) -> [VariableInput] {
        values.sorted { $0.key < $1.key }.map { .plain($0.key, $0.value) }
    }
}

/// Which layer of a Run's environment variables belong to.
public enum VariableLayer: Sendable, Hashable, Codable {
    case project(UUID)
    case service(UUID)
    case profile(UUID)

    var ownerID: UUID {
        switch self {
        case .project(let id), .service(let id), .profile(let id): id
        }
    }

    /// Where the layer's Secret `name` is stored, e.g. `<service-uuid>/API_KEY`.
    func secretAccount(_ name: String) -> String {
        "\(ownerID.uuidString)/\(name)"
    }
}

/// A named set of environment variables for a Project (e.g. Development,
/// Testing) applying to all its Services; at most one is active at a time.
public struct EnvironmentProfile: Identifiable, Hashable, Sendable, Codable {
    public let id: UUID
    public internal(set) var name: String
    public internal(set) var variables: Variables

    init(id: UUID = UUID(), name: String, variables: Variables = .empty) {
        self.id = id
        self.name = name
        self.variables = variables
    }
}

// MARK: Secret storage

/// Where Secret values live. Each Secret is one item, addressed by its
/// layer's owner and its name.
public protocol SecretStore: Sendable {
    func value(for account: String) throws(LocalhostError) -> String?
    func setValue(_ value: String, for account: String) throws(LocalhostError)
    func removeValue(for account: String) throws(LocalhostError)
}

/// Stores each Secret as one generic-password item in the login Keychain.
public struct KeychainSecretStore: SecretStore {
    public static let service = "com.marcellosso.localhost.secret"

    public init() {}

    public func value(for account: String) throws(LocalhostError) -> String? {
        var query = baseQuery(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else { throw .keychainFailed(status: status) }
        return String(decoding: data, as: UTF8.self)
    }

    public func setValue(_ value: String, for account: String) throws(LocalhostError) {
        let data = Data(value.utf8)
        let status = SecItemUpdate(baseQuery(account) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if status == errSecItemNotFound {
            var item = baseQuery(account)
            item[kSecValueData as String] = data
            item[kSecAttrLabel as String] = "localhost secret"
            let addStatus = SecItemAdd(item as CFDictionary, nil)
            guard addStatus == errSecSuccess else { throw .keychainFailed(status: addStatus) }
        } else if status != errSecSuccess {
            throw .keychainFailed(status: status)
        }
    }

    public func removeValue(for account: String) throws(LocalhostError) {
        let status = SecItemDelete(baseQuery(account) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw .keychainFailed(status: status) }
    }

    private func baseQuery(_ account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account,
        ]
    }
}
