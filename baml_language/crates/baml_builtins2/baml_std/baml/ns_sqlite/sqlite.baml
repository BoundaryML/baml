/// Execute one statement against an in-process SQLite connection. Parameters
/// are a JSON array of null, integer, float or string. Returns compact JSON
/// {"rows": [[...]], "changes": int}; statements are stepped to completion
/// before returning, including INSERT ... RETURNING (autocommit is durable).
/// Connections and prepared statements are reused, never query results.
function query(path: string, sql: string, params_json: string) -> string throws root.errors.Io {
    $rust_io_function
}
