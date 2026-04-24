import Foundation
import Security

struct AICredentialStore {
    private let service = "dmux.ai.providers"

    func apiKey(for reference: String?) -> String? {
        guard let reference = normalizedReference(reference) else {
            return nil
        }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: reference,
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

    func saveAPIKey(_ value: String, for reference: String) {
        let normalizedValue = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalizedReference = normalizedReference(reference)
        guard let normalizedReference else {
            return
        }

        if normalizedValue.isEmpty {
            deleteAPIKey(for: normalizedReference)
            return
        }

        let passwordData = Data(normalizedValue.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: normalizedReference,
        ]
        let attributes: [String: Any] = [
            kSecValueData as String: passwordData,
        ]

        let status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            var insert = query
            attributes.forEach { insert[$0.key] = $0.value }
            SecItemAdd(insert as CFDictionary, nil)
        }
    }

    func deleteAPIKey(for reference: String?) {
        guard let reference = normalizedReference(reference) else {
            return
        }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: reference,
        ]
        SecItemDelete(query as CFDictionary)
    }

    func defaultReference(for providerID: String) -> String {
        "provider:\(providerID)"
    }

    private func normalizedReference(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }
}
