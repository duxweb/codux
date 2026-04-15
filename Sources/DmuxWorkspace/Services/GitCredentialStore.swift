import Foundation
import Security

struct GitCredentialStore {
    private let service = "dmux.git"

    func credential(for remoteURL: String) -> GitCredential? {
        let account = normalizedAccount(for: remoteURL)
        guard let password = readPassword(account: account),
              let username = readAttribute(account: account, key: kSecAttrLabel as String) else {
            return nil
        }
        return GitCredential(username: username, password: password)
    }

    func save(_ credential: GitCredential, for remoteURL: String) {
        let account = normalizedAccount(for: remoteURL)
        let passwordData = Data(credential.password.utf8)

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]

        let attributes: [String: Any] = [
            kSecAttrLabel as String: credential.username,
            kSecValueData as String: passwordData,
        ]

        let status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            var insert = query
            attributes.forEach { insert[$0.key] = $0.value }
            SecItemAdd(insert as CFDictionary, nil)
        }
    }

    private func normalizedAccount(for remoteURL: String) -> String {
        remoteURL.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func readPassword(account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess,
              let data = item as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    private func readAttribute(account: String, key: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnAttributes as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess,
              let attributes = item as? [String: Any] else {
            return nil
        }
        return attributes[key] as? String
    }
}
