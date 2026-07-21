enum SqliteOpenMode {
  ReadOnly
  ReadWrite
  ReadWriteCreate
}

enum SqliteJournalMode {
  Delete
  Truncate
  Persist
  Memory
  Wal
  Off
}

class SqliteOptions {
  mode SqliteOpenMode?
  busy_timeout root.time.Duration?
  query_timeout root.time.Duration?
  journal_mode SqliteJournalMode?
  foreign_keys bool?
  max_connections int?
  validate_connection bool?
}

function open(path: string, options: SqliteOptions? = null) -> root.sql.Database throws root.sql.SqlError {
  _open_sqlite(path, options) catch (_) {
    let e: root.sql.SqlError => throw e,
    root.errors.Unsupported { message } => throw root.sql.SqlError { kind: root.sql.SqlErrorKind.Unsupported, message: message, code: null, constraint: null, table: null, column: null },
  }
}

function _open_sqlite(path: string, options: SqliteOptions?) -> root.sql.Database throws root.sql.SqlError | root.errors.Unsupported {
  $rust_io_function
}

function memory(options: SqliteOptions? = null) -> root.sql.Database throws root.sql.SqlError {
  _memory_sqlite(options) catch (_) {
    let e: root.sql.SqlError => throw e,
    root.errors.Unsupported { message } => throw root.sql.SqlError { kind: root.sql.SqlErrorKind.Unsupported, message: message, code: null, constraint: null, table: null, column: null },
  }
}

function _memory_sqlite(options: SqliteOptions?) -> root.sql.Database throws root.sql.SqlError | root.errors.Unsupported {
  $rust_io_function
}
