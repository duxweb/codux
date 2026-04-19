import Foundation
import SQLite3

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
