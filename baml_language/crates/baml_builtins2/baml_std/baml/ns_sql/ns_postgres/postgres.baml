enum PostgresSslMode {
  Disable
  Allow
  Prefer
  Require
  VerifyCa
  VerifyFull
}

class PostgresOptions {
  max_connections int?
  min_connections int?
  connect_timeout root.time.Duration?
  query_timeout root.time.Duration?
  idle_timeout root.time.Duration?
  max_lifetime root.time.Duration?
  application_name string?
  ssl_mode PostgresSslMode?
  validate bool?
}

function connect(url: string, options: PostgresOptions? = null) -> root.sql.Database throws root.sql.SqlError {
  _connect_postgres(url, options) catch (_) {
    let e: root.sql.SqlError => throw e,
    root.errors.Unsupported { message } => throw root.sql.SqlError { kind: root.sql.SqlErrorKind.Unsupported, message: message, code: null, constraint: null, table: null, column: null },
  }
}

function _connect_postgres(url: string, options: PostgresOptions?) -> root.sql.Database throws root.sql.SqlError | root.errors.Unsupported {
  $rust_io_function
}
