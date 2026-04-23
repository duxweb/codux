import CryptoKit
import Foundation
import SQLite3

func normalizedNonEmptyString(_ value: String?) -> String? {
    guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
          !value.isEmpty else {
        return nil
    }
    return value
}

func deterministicUUID(from value: String) -> UUID {
    let digest = SHA256.hash(data: Data(value.utf8))
    let bytes = Array(digest.prefix(16))
    let uuidBytes: uuid_t = (
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11],
        bytes[12], bytes[13], bytes[14], bytes[15]
    )
    return UUID(uuid: uuidBytes)
}

enum SQLiteBindingValue {
    case text(String)
    case int64(Int64)
}

let SQLITE_TRANSIENT_SESSION = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

func withSQLiteDatabase(path: String, body: (OpaquePointer) throws -> Void) throws {
    var db: OpaquePointer?
    guard sqlite3_open(path, &db) == SQLITE_OK, let db else {
        defer {
            if db != nil {
                sqlite3_close(db)
            }
        }
        throw AIToolSessionControlError.storageFailure(String(localized: "ai.session.storage.open_failed", defaultValue: "Unable to open session storage.", bundle: .module))
    }
    defer { sqlite3_close(db) }
    try body(db)
}

func executeSQLite(db: OpaquePointer, sql: String, bindings: [SQLiteBindingValue]) throws {
    var statement: OpaquePointer?
    guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
        throw AIToolSessionControlError.storageFailure(String(cString: sqlite3_errmsg(db)))
    }
    defer { sqlite3_finalize(statement) }

    for (index, binding) in bindings.enumerated() {
        let position = Int32(index + 1)
        switch binding {
        case let .text(value):
            sqlite3_bind_text(statement, position, value, -1, SQLITE_TRANSIENT_SESSION)
        case let .int64(value):
            sqlite3_bind_int64(statement, position, value)
        }
    }

    let result = sqlite3_step(statement)
    guard result == SQLITE_DONE else {
        throw AIToolSessionControlError.storageFailure(String(cString: sqlite3_errmsg(db)))
    }
}

func shellQuoted(_ value: String) -> String {
    "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
}
