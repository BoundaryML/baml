enum SqlIsolation {
  ReadCommitted
  RepeatableRead
  Serializable
}

enum SqlErrorKind {
  Connection
  Closed
  Timeout
  Syntax
  Constraint
  NotFound
  TooManyRows
  Decode
  Unsupported
  Database
}

class SqlError {
  kind SqlErrorKind
  message string
  code string?
  constraint string?
  table string?
  column string?
}

class ConnectOptions {
  max_connections int?
  connect_timeout root.time.Duration?
  query_timeout root.time.Duration?
  validate bool?
}

class QueryOptions {
  timeout root.time.Duration?
}

class TransactionOptions {
  isolation SqlIsolation?
  read_only bool?
}

class CommandResult {
  rows_affected int
}

class JsonValue {
  value root.json.json
}

function json(value: root.json.json) -> JsonValue throws never {
  JsonValue { value: value }
}

class Statement {
  _handle $rust_type
}

//baml:tagged_string
function statement(body: () -> root.TaggedString throws never) -> Statement throws SqlError {
  let value = body()
  _statement(value.parts, value.values)
}

//baml:mut_vm
function _statement(parts: string[], values: unknown[]) -> Statement throws SqlError {
  $rust_function
}

function _duration_nanos(value: root.time.Duration?) -> bigint? throws never {
  match (value) {
    null => null,
    let duration: root.time.Duration => duration.to_nanoseconds(),
  }
}

class Database {
  _handle $rust_type

  function close(self) -> null throws SqlError {
    self._close()
  }

  function _close(self) -> null throws SqlError {
    $rust_io_function
  }

  function execute(self, statement: Statement, options: QueryOptions? = null) -> CommandResult throws SqlError {
    self._execute(statement, _query_timeout(options))
  }

  function _execute(self, statement: Statement, timeout_nanos: bigint?) -> CommandResult throws SqlError {
    $rust_io_function
  }

  function query<T>(self, statement: Statement, options: QueryOptions? = null) -> T[] throws SqlError {
    self._query<T>(statement, _query_timeout(options), null)
  }

  function query_optional<T>(self, statement: Statement, options: QueryOptions? = null) -> T? throws SqlError {
    let rows = self._query<T>(statement, _query_timeout(options), 2)
    match (rows.length()) {
      0 => null,
      1 => rows[0],
      _ => throw SqlError { kind: SqlErrorKind.TooManyRows, message: "query returned more than one row", code: null, constraint: null, table: null, column: null },
    }
  }

  function query_one<T>(self, statement: Statement, options: QueryOptions? = null) -> T throws SqlError {
    let rows = self._query<T>(statement, _query_timeout(options), 2)
    match (rows.length()) {
      0 => throw SqlError { kind: SqlErrorKind.NotFound, message: "query returned no rows", code: null, constraint: null, table: null, column: null },
      1 => rows[0],
      _ => throw SqlError { kind: SqlErrorKind.TooManyRows, message: "query returned more than one row", code: null, constraint: null, table: null, column: null },
    }
  }

  function scalar<T>(self, statement: Statement, options: QueryOptions? = null) -> T throws SqlError {
    self._scalar<T>(statement, _query_timeout(options), 2)
  }

  function _query<T>(self, statement: Statement, timeout_nanos: bigint?, row_limit: int?) -> T[] throws SqlError {
    $rust_io_function
  }

  function _scalar<T>(self, statement: Statement, timeout_nanos: bigint?, row_limit: int) -> T throws SqlError {
    $rust_io_function
  }

  function transaction<T, E>(self, body: (Transaction) -> T throws E, options: TransactionOptions? = null) -> T throws SqlError | E {
    let tx = self._begin(options)
    defer { tx._rollback_if_open() }
    let value = body(tx)
    tx._commit()
    value
  }

  function _begin(self, options: TransactionOptions?) -> Transaction throws SqlError {
    $rust_io_function
  }
}

class Transaction {
  _handle $rust_type

  function execute(self, statement: Statement, options: QueryOptions? = null) -> CommandResult throws SqlError {
    self._execute(statement, _query_timeout(options))
  }

  function _execute(self, statement: Statement, timeout_nanos: bigint?) -> CommandResult throws SqlError {
    $rust_io_function
  }

  function query<T>(self, statement: Statement, options: QueryOptions? = null) -> T[] throws SqlError {
    self._query<T>(statement, _query_timeout(options), null)
  }

  function query_optional<T>(self, statement: Statement, options: QueryOptions? = null) -> T? throws SqlError {
    let rows = self._query<T>(statement, _query_timeout(options), 2)
    match (rows.length()) {
      0 => null,
      1 => rows[0],
      _ => throw SqlError { kind: SqlErrorKind.TooManyRows, message: "query returned more than one row", code: null, constraint: null, table: null, column: null },
    }
  }

  function query_one<T>(self, statement: Statement, options: QueryOptions? = null) -> T throws SqlError {
    let rows = self._query<T>(statement, _query_timeout(options), 2)
    match (rows.length()) {
      0 => throw SqlError { kind: SqlErrorKind.NotFound, message: "query returned no rows", code: null, constraint: null, table: null, column: null },
      1 => rows[0],
      _ => throw SqlError { kind: SqlErrorKind.TooManyRows, message: "query returned more than one row", code: null, constraint: null, table: null, column: null },
    }
  }

  function scalar<T>(self, statement: Statement, options: QueryOptions? = null) -> T throws SqlError {
    self._scalar<T>(statement, _query_timeout(options), 2)
  }

  function _query<T>(self, statement: Statement, timeout_nanos: bigint?, row_limit: int?) -> T[] throws SqlError {
    $rust_io_function
  }

  function _scalar<T>(self, statement: Statement, timeout_nanos: bigint?, row_limit: int) -> T throws SqlError {
    $rust_io_function
  }

  function _commit(self) -> null throws SqlError {
    $rust_io_function
  }

  function _rollback_if_open(self) -> null throws SqlError {
    $rust_io_function
  }
}

function _query_timeout(options: QueryOptions?) -> bigint? throws never {
  match (options) {
    null => null,
    let value: QueryOptions => _duration_nanos(value.timeout),
  }
}

function connect(url: string, options: ConnectOptions? = null) -> Database throws SqlError {
  _connect(url, options) catch (_) {
    let e: SqlError => throw e,
    root.errors.Unsupported { message } => throw SqlError { kind: SqlErrorKind.Unsupported, message: message, code: null, constraint: null, table: null, column: null },
  }
}

function _connect(url: string, options: ConnectOptions?) -> Database throws SqlError | root.errors.Unsupported {
  $rust_io_function
}
