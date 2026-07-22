//! End-to-end coverage for the native `baml.sql` standard library.

use baml_tests::baml_test;
use bex_external_types::BexExternalValue;

fn sql_error_kind(result: &Result<BexExternalValue, bex_engine::EngineError>) -> &str {
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = result else {
        panic!("expected an unhandled SqlError, got {result:?}");
    };
    let BexExternalValue::Instance {
        class_name, fields, ..
    } = value.as_ref()
    else {
        panic!("expected SqlError instance, got {value:?}");
    };
    assert_eq!(class_name, "baml.sql.SqlError");
    let Some(BexExternalValue::Variant { variant_name, .. }) = fields.get("kind") else {
        panic!("SqlError has no kind: {fields:?}");
    };
    variant_name
}

#[tokio::test]
async fn sqlite_statement_binding_and_typed_rows() {
    let output = baml_test!(
        r#"
class UserRow {
  id int
  name string
  active bool
}

function main() -> UserRow {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.execute(baml.sql.statement`CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT, active INTEGER)`)
  let name = "O'Reilly -- ? $1"
  db.execute(baml.sql.statement`INSERT INTO users(id, name, active) VALUES (${7}, ${name}, ${true})`)
  db.query_one<UserRow>(baml.sql.statement`SELECT id, name, active FROM users`)
}
"#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Instance {
            class_name: "user.UserRow".to_string(),
            type_args: vec![],
            fields: indexmap::indexmap! {
                "id".to_string() => BexExternalValue::Int(7),
                "name".to_string() => BexExternalValue::String("O'Reilly -- ? $1".into()),
                "active".to_string() => BexExternalValue::Bool(true),
            },
        })
    );
}

#[tokio::test]
async fn sqlite_round_trips_every_supported_bind_encoding() {
    let output = baml_test!(
        r#"
class Values {
  nil string?
  flag bool
  small int
  huge bigint
  real float
  text string
  bytes uint8array
  payload baml.json.json
  instant baml.time.Instant
  zoned baml.time.ZonedDateTime
  zoned_encoding string
  datetime baml.time.PlainDateTime
  date baml.time.PlainDate
  time baml.time.PlainTime
  duration baml.time.Duration
}

function main() -> Values {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.execute(baml.sql.statement`CREATE TABLE vals(
    nil TEXT, flag INTEGER, small INTEGER, huge TEXT, real REAL, text TEXT,
    bytes BLOB, payload TEXT, instant TEXT, zoned TEXT, datetime TEXT,
    date TEXT, time TEXT, duration TEXT
  )`)
  let payload = baml.sql.json(baml.json.parse("{\"answer\":42}"))
  let instant = baml.time.Instant.parse("2024-01-02T03:04:05.123456789Z")
  let zoned = baml.time.ZonedDateTime.parse("2024-01-02T04:04:05.123456789+01:00[Europe/Paris]")
  let datetime = baml.time.PlainDateTime.parse("2024-01-02T03:04:05.123456789")
  let date = baml.time.PlainDate.parse("2024-01-02")
  let time = baml.time.PlainTime.parse("03:04:05.123456789")
  let duration = baml.time.Duration.from_nanoseconds(-123456789n)
  db.execute(baml.sql.statement`INSERT INTO vals VALUES (
    ${null}, ${true}, ${-7}, ${999999999999999999999999n}, ${1.25}, ${"hello"},
    ${"az".to_utf8()}, ${payload}, ${instant}, ${zoned}, ${datetime}, ${date}, ${time}, ${duration}
  )`)
  db.query_one<Values>(baml.sql.statement`SELECT *, zoned AS zoned_encoding FROM vals`)
}
"#
    );
    let value = output.result.unwrap();
    let BexExternalValue::Instance { fields, .. } = value else {
        panic!("expected Values instance");
    };
    assert_eq!(fields["nil"], BexExternalValue::Null);
    assert_eq!(fields["flag"], BexExternalValue::Bool(true));
    assert_eq!(fields["small"], BexExternalValue::Int(-7));
    assert_eq!(
        fields["huge"],
        BexExternalValue::Bigint("999999999999999999999999".parse().unwrap())
    );
    assert_eq!(fields["real"], BexExternalValue::Float(1.25));
    assert_eq!(fields["text"], BexExternalValue::String("hello".into()));
    assert_eq!(
        fields["zoned_encoding"],
        BexExternalValue::String("2024-01-02T04:04:05.123456789+01:00[Europe/Paris]".into())
    );
    assert_eq!(
        fields["bytes"],
        BexExternalValue::Uint8Array(vec![b'a', b'z'])
    );
    assert!(matches!(fields["payload"], BexExternalValue::Map { .. }));
    for name in ["instant", "zoned", "datetime", "date", "time", "duration"] {
        assert!(
            matches!(&fields[name], BexExternalValue::Instance { .. }),
            "{name}: {:?}",
            fields[name]
        );
    }
    let BexExternalValue::Instance { fields: zoned, .. } = &fields["zoned"] else {
        unreachable!()
    };
    assert_eq!(zoned["_offset_ns"], BexExternalValue::Null);
    assert_eq!(
        zoned["_iana"],
        BexExternalValue::String("Europe/Paris".into())
    );
}

#[tokio::test]
async fn sqlite_cardinality_methods_are_bounded_and_exact() {
    let output = baml_test!(
        r#"
function label(error: baml.sql.SqlError) -> string {
  match (error.kind) {
    baml.sql.SqlErrorKind.NotFound => "not-found",
    baml.sql.SqlErrorKind.TooManyRows => "too-many",
    baml.sql.SqlErrorKind.Decode => "decode",
    _ => "other",
  }
}

function main() -> string {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  let optional = db.query_optional<int>(baml.sql.statement`SELECT 1 WHERE false`)
  let a = match (optional) { null => "none", _ => "some" }
  let b = db.query_one<int>(baml.sql.statement`SELECT 1 WHERE false`) catch (e) {
    let error: baml.sql.SqlError => label(error)
  }
  let c = db.query_optional<int>(baml.sql.statement`SELECT 1 UNION ALL SELECT 2`) catch (e) {
    let error: baml.sql.SqlError => label(error)
  }
  let d = db.scalar<int>(baml.sql.statement`SELECT 1, 2`) catch (e) {
    let error: baml.sql.SqlError => label(error)
  }
  a + ":" + b.to_string() + ":" + c.to_string() + ":" + d.to_string()
}
"#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "none:not-found:too-many:decode".into()
        ))
    );
}

#[tokio::test]
async fn sqlite_transactions_commit_rollback_read_only_and_invalidate_handles() {
    let output = baml_test!(
        r#"
function main() -> string {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.execute(baml.sql.statement`CREATE TABLE events(value TEXT)`)
  db.transaction((tx) -> {
    tx.execute(baml.sql.statement`INSERT INTO events VALUES (${"committed"})`)
    tx.scalar<int>(baml.sql.statement`SELECT count(*) FROM events`)
  })
  db.transaction((tx) -> {
    tx.execute(baml.sql.statement`INSERT INTO events VALUES (${"rolled-back"})`)
    throw "stop"
  }) catch (_) { _ => null }
  let leaked = db.transaction((tx) -> { tx })
  let closed = leaked.scalar<int>(baml.sql.statement`SELECT 1`) catch (e) {
    let error: baml.sql.SqlError => match (error.kind) { baml.sql.SqlErrorKind.Closed => "closed", _ => "wrong" }
  }
  let readonly = db.transaction(
    (tx) -> {
      tx.execute(baml.sql.statement`INSERT INTO events VALUES (${"forbidden"})`)
      "wrong"
    },
    options = baml.sql.TransactionOptions { read_only: true },
  ) catch (e) {
    let error: baml.sql.SqlError => match (error.kind) { baml.sql.SqlErrorKind.Database => "readonly", _ => "wrong" }
  }
  db.execute(baml.sql.statement`CREATE TABLE parents(id INTEGER PRIMARY KEY)`)
  db.execute(baml.sql.statement`CREATE TABLE children(
    parent_id INTEGER REFERENCES parents(id) DEFERRABLE INITIALLY DEFERRED
  )`)
  let commit_failure = db.transaction((tx) -> {
    tx.execute(baml.sql.statement`INSERT INTO children VALUES (${1})`)
    "wrong"
  }) catch (e) {
    let error: baml.sql.SqlError => match (error.kind) { baml.sql.SqlErrorKind.Constraint => "rolled-back", _ => "wrong" }
  }
  let child_count = db.scalar<int>(baml.sql.statement`SELECT count(*) FROM children`)
  let count = db.scalar<int>(baml.sql.statement`SELECT count(*) FROM events`)
  closed.to_string() + ":" + readonly + ":" + commit_failure + ":" + child_count.to_string() + ":" + count.to_string()
}
"#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "closed:readonly:rolled-back:0:1".into()
        ))
    );
}

#[tokio::test]
async fn sqlite_transaction_timeout_discards_connection_and_poisons_handle() {
    let output = baml_test!(
        r#"
function main() -> string {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  let tx = db._begin(null)
  let timed_out = tx.query<int>(
    baml.sql.statement`WITH RECURSIVE n(x) AS (
      SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 100000000
    ) SELECT x FROM n`,
    options = baml.sql.QueryOptions { timeout: baml.time.Duration.from_milliseconds(10) },
  ) catch (e) {
    let error: baml.sql.SqlError => match (error.kind) {
      baml.sql.SqlErrorKind.Timeout => "timeout",
      _ => "wrong",
    }
  }
  let closed = tx.scalar<int>(baml.sql.statement`SELECT 1`) catch (e) {
    let error: baml.sql.SqlError => match (error.kind) {
      baml.sql.SqlErrorKind.Closed => "closed",
      _ => "wrong",
    }
  }
  tx._rollback_if_open()
  let pool_value = db.scalar<int>(baml.sql.statement`SELECT 1`)
  timed_out.to_string() + ":" + closed.to_string() + ":" + pool_value.to_string()
}
"#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("timeout:closed:1".into()))
    );
}

#[tokio::test]
async fn sqlite_structural_decode_honors_alias_optional_and_extra_columns() {
    let output = baml_test!(
        r#"
class Row {
  renamed string @alias("db_name")
  optional int?
}
function main() -> Row {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.query_one<Row>(baml.sql.statement`SELECT 'ok' AS db_name, 9 AS ignored`)
}
"#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Instance {
            class_name: "user.Row".into(),
            type_args: vec![],
            fields: indexmap::indexmap! {
                "renamed".into() => BexExternalValue::String("ok".into()),
                "optional".into() => BexExternalValue::Null,
            },
        })
    );
}

#[tokio::test]
async fn sqlite_duplicate_or_missing_required_columns_are_decode_errors() {
    for select in ["SELECT 1 AS value, 2 AS value", "SELECT 1 AS unrelated"] {
        let output = baml_test!(&format!(
            r#"
class Row {{ value int }}
function main() -> Row {{
  let db = baml.sql.sqlite.memory()
  defer {{ db.close() }}
  db.query_one<Row>(baml.sql.statement`{select}`)
}}
"#
        ));
        assert_eq!(sql_error_kind(&output.result), "Decode");
    }

    for value in ["+1", "01", "-0"] {
        let output = baml_test!(&format!(
            r#"
function main() -> bigint {{
  let db = baml.sql.sqlite.memory()
  defer {{ db.close() }}
  db.scalar<bigint>(baml.sql.statement`SELECT '{value}'`)
}}
"#
        ));
        assert_eq!(sql_error_kind(&output.result), "Decode", "{value}");
    }
}

#[tokio::test]
async fn sqlite_rejects_unsupported_binds_and_execute_returning() {
    let array = baml_test!(
        r#"
function main() -> null {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.execute(baml.sql.statement`SELECT ${[1, 2]}`)
  null
}
"#
    );
    assert_eq!(sql_error_kind(&array.result), "Unsupported");

    let returning = baml_test!(
        r#"
function main() -> null {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.execute(baml.sql.statement`SELECT 1`)
  null
}
"#
    );
    assert_eq!(sql_error_kind(&returning.result), "Database");
}

#[tokio::test]
async fn sqlite_close_is_idempotent_and_prevents_new_work() {
    let output = baml_test!(
        r#"
function main() -> int {
  let db = baml.sql.sqlite.memory()
  db.close()
  db.close()
  db.scalar<int>(baml.sql.statement`SELECT 1`)
}
"#
    );
    assert_eq!(sql_error_kind(&output.result), "Closed");
}

#[tokio::test]
async fn tagged_statement_evaluates_interpolations_once_in_source_order() {
    let output = baml_test!(
        r#"
function main() -> string {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  let values = [1, 2]
  db.scalar<string>(baml.sql.statement`SELECT ${values.pop()} || ':' || ${values.pop()}`)
}
"#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("2:1".into())));
}

#[tokio::test]
async fn sqlite_file_database_works_through_common_connect_dispatch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("common-connect.db");
    let url = format!("sqlite:{}", path.display()).replace('\\', "/");
    let output = baml_test!(&format!(
        r#"
function main() -> string {{
  let db = baml.sql.connect("{url}")
  db.execute(baml.sql.statement`CREATE TABLE item(value TEXT)`)
  db.execute(baml.sql.statement`INSERT INTO item VALUES (${{"persisted"}})`)
  db.close()
  let reopened = baml.sql.sqlite.open(
    "{url}",
    options = baml.sql.sqlite.SqliteOptions {{ mode: baml.sql.sqlite.SqliteOpenMode.ReadOnly }},
  )
  defer {{ reopened.close() }}
  reopened.scalar<string>(baml.sql.statement`SELECT value FROM item`)
}}
"#
    ));
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("persisted".into()))
    );

    for url in ["relative.db", "mysql://localhost/example"] {
        let output = baml_test!(&format!(
            r#"
function main() -> null {{
  baml.sql.connect("{url}")
  null
}}
"#
        ));
        assert_eq!(sql_error_kind(&output.result), "Connection", "{url}");
    }
}

#[tokio::test]
async fn sqlite_validates_memory_options_and_isolation() {
    let pooled_memory = baml_test!(
        r#"
function main() -> null {
  baml.sql.sqlite.memory(options = baml.sql.sqlite.SqliteOptions { max_connections: 2 })
  null
}
"#
    );
    assert_eq!(sql_error_kind(&pooled_memory.result), "Connection");

    let wal_memory = baml_test!(
        r#"
function main() -> null {
  baml.sql.sqlite.memory(options = baml.sql.sqlite.SqliteOptions {
    journal_mode: baml.sql.sqlite.SqliteJournalMode.Wal,
  })
  null
}
"#
    );
    assert_eq!(sql_error_kind(&wal_memory.result), "Connection");

    let shared_memory = baml_test!(
        r#"
function main() -> null {
  baml.sql.connect("file:baml-memory?mode=memory&cache=shared")
  null
}
"#
    );
    assert_eq!(sql_error_kind(&shared_memory.result), "Connection");

    let isolation = baml_test!(
        r#"
function main() -> null {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.transaction(
    (tx) -> { null },
    options = baml.sql.TransactionOptions {
      isolation: baml.sql.SqlIsolation.ReadCommitted,
    },
  )
}
"#
    );
    assert_eq!(sql_error_kind(&isolation.result), "Unsupported");
}

#[tokio::test]
async fn sqlite_maps_syntax_constraint_and_timeout_errors() {
    let syntax = baml_test!(
        r#"
function main() -> int {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.scalar<int>(baml.sql.statement`SELEC 1`)
}
"#
    );
    assert_eq!(sql_error_kind(&syntax.result), "Syntax");

    let constraint = baml_test!(
        r#"
function main() -> null {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.execute(baml.sql.statement`CREATE TABLE unique_values(value INTEGER UNIQUE)`)
  db.execute(baml.sql.statement`INSERT INTO unique_values VALUES (1)`)
  db.execute(baml.sql.statement`INSERT INTO unique_values VALUES (1)`)
  null
}
"#
    );
    assert_eq!(sql_error_kind(&constraint.result), "Constraint");
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = constraint.result else {
        unreachable!()
    };
    let BexExternalValue::Instance { fields, .. } = *value else {
        unreachable!()
    };
    assert_eq!(
        fields["code"],
        BexExternalValue::String("SQLITE_CONSTRAINT_UNIQUE".into())
    );

    let timeout = baml_test!(
        r#"
function main() -> int {
  let db = baml.sql.sqlite.memory()
  defer { db.close() }
  db.scalar<int>(
    baml.sql.statement`WITH RECURSIVE n(x) AS (
      SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 1000000
    ) SELECT sum(x) FROM n`,
    options = baml.sql.QueryOptions { timeout: baml.time.Duration.from_nanoseconds(1) },
  )
}
"#
    );
    assert_eq!(sql_error_kind(&timeout.result), "Timeout");
}

#[tokio::test]
async fn postgres_bind_decode_array_enum_and_time_matrix() {
    let Ok(url) = std::env::var("BAML_TEST_POSTGRES_URL") else {
        eprintln!("skipping PostgreSQL SQL test: BAML_TEST_POSTGRES_URL is unset");
        return;
    };
    let output = baml_test! {
        baml: r#"
class PgValues {
  flag bool
  i2 int
  i4 int
  i8 int
  numeric_int bigint
  numeric_text string
  real float
  double float
  text string
  bytes uint8array
  payload baml.json.json
  uuid string
  date baml.time.PlainDate
  time baml.time.PlainTime
  datetime baml.time.PlainDateTime
  instant baml.time.Instant
  zoned baml.time.ZonedDateTime
  duration baml.time.Duration
  ints int[]
  empty int[]
}

function main(url: string) -> PgValues {
  let db = baml.sql.postgres.connect(url)
  defer { db.close() }
  let payload = baml.sql.json(baml.json.parse("{\"provider\":\"postgres\"}"))
  let date = baml.time.PlainDate.parse("2024-02-03")
  let time = baml.time.PlainTime.parse("04:05:06.123456789")
  let datetime = baml.time.PlainDateTime.parse("2024-02-03T04:05:06.123456789")
  let instant = baml.time.Instant.parse("2024-02-03T04:05:06.123456789Z")
  let zoned = baml.time.ZonedDateTime.parse("2024-02-03T05:05:06.123456789+01:00[Europe/Paris]")
  let duration = baml.time.Duration.from_nanoseconds(123456789n)
  let ints: int[] = [1, 2, 3]
  let empty: int[] = []
  db.query_one<PgValues>(baml.sql.statement`SELECT
    ${true}::boolean AS flag,
    ${-2}::smallint AS i2,
    ${3}::integer AS i4,
    ${4}::bigint AS i8,
    ${999999999999999999999999n}::numeric AS numeric_int,
    123.4500::numeric AS numeric_text,
    1.25::real AS real,
    ${2.5}::double precision AS double,
    ${"safe ' text -- $1"}::text AS text,
    ${"az".to_utf8()}::bytea AS bytes,
    ${payload}::jsonb AS payload,
    ${"550e8400-e29b-41d4-a716-446655440000"}::uuid AS uuid,
    ${date}::date AS date,
    ${time}::time AS time,
    ${datetime}::timestamp AS datetime,
    ${instant}::timestamptz AS instant,
    ${zoned}::timestamptz AS zoned,
    ${duration}::interval AS duration,
    ${ints}::bigint[] AS ints,
    ${empty}::bigint[] AS empty`)
}
"#,
        args: { "url" => BexExternalValue::String(url.clone().into()) },
    };
    let value = output.result.unwrap();
    let BexExternalValue::Instance { fields, .. } = value else {
        panic!("expected PgValues instance");
    };
    assert_eq!(fields["flag"], BexExternalValue::Bool(true));
    assert_eq!(fields["i2"], BexExternalValue::Int(-2));
    assert_eq!(
        fields["numeric_text"],
        BexExternalValue::String("123.45".into())
    );
    assert_eq!(
        fields["uuid"],
        BexExternalValue::String("550e8400-e29b-41d4-a716-446655440000".into())
    );
    assert!(matches!(&fields["payload"], BexExternalValue::Map { .. }));
    assert!(matches!(
        &fields["empty"],
        BexExternalValue::Array { items, .. } if items.is_empty()
    ));

    let fractional_numeric_array = baml_test! {
        baml: r#"
function main(url: string) -> bigint[] {
  let db = baml.sql.postgres.connect(url)
  defer { db.close() }
  db.scalar<bigint[]>(baml.sql.statement`SELECT ARRAY[1.5::numeric]`)
}
"#,
        args: { "url" => BexExternalValue::String(url.into()) },
    };
    assert_eq!(sql_error_kind(&fractional_numeric_array.result), "Decode");
}

#[tokio::test]
async fn postgres_transactions_returning_enum_and_sqlstate_errors() {
    let Ok(url) = std::env::var("BAML_TEST_POSTGRES_URL") else {
        eprintln!("skipping PostgreSQL SQL test: BAML_TEST_POSTGRES_URL is unset");
        return;
    };
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let table = format!("baml_sql_test_{suffix}");
    let enum_name = format!("baml_sql_mood_{suffix}");
    let source = format!(
        r#"
function main(url: string) -> string {{
  let db = baml.sql.postgres.connect(url)
  defer {{ db.close() }}
  db.execute(baml.sql.statement`CREATE TYPE {enum_name} AS ENUM ('happy', 'sad')`)
  defer {{ db.execute(baml.sql.statement`DROP TYPE IF EXISTS {enum_name}`) }}
  db.execute(baml.sql.statement`CREATE TABLE {table}(id BIGINT PRIMARY KEY, mood {enum_name})`)
  defer {{ db.execute(baml.sql.statement`DROP TABLE IF EXISTS {table}`) }}
  let id = db.scalar<int>(baml.sql.statement`INSERT INTO {table} VALUES (${{1}}, 'happy') RETURNING id`)
  db.transaction(
    (tx) -> {{ tx.execute(baml.sql.statement`INSERT INTO {table} VALUES (${{2}}, 'sad')`) }},
    options = baml.sql.TransactionOptions {{ isolation: baml.sql.SqlIsolation.Serializable }},
  )
  db.transaction((tx) -> {{
    tx.execute(baml.sql.statement`INSERT INTO {table} VALUES (${{3}}, 'happy')`)
    throw "rollback"
  }}) catch (_) {{ _ => null }}
  let mood = db.scalar<string>(baml.sql.statement`SELECT mood FROM {table} WHERE id = ${{1}}`)
  let count = db.scalar<int>(baml.sql.statement`SELECT count(*) FROM {table}`)
  id.to_string() + ":" + mood + ":" + count.to_string()
}}
"#
    );
    let output = baml_test! {
        baml: &source,
        args: { "url" => BexExternalValue::String(url.clone().into()) },
    };
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("1:happy:2".into()))
    );

    let constraint_source = format!(
        r#"
function main(url: string) -> null {{
  let db = baml.sql.postgres.connect(url)
  defer {{ db.close() }}
  db.execute(baml.sql.statement`CREATE TABLE {table}(id BIGINT PRIMARY KEY)`)
  defer {{ db.execute(baml.sql.statement`DROP TABLE IF EXISTS {table}`) }}
  db.execute(baml.sql.statement`INSERT INTO {table} VALUES (1)`)
  db.execute(baml.sql.statement`INSERT INTO {table} VALUES (1)`)
  null
}}
"#
    );
    let constraint = baml_test! {
        baml: &constraint_source,
        args: { "url" => BexExternalValue::String(url.into()) },
    };
    assert_eq!(sql_error_kind(&constraint.result), "Constraint");
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = constraint.result else {
        unreachable!()
    };
    let BexExternalValue::Instance { fields, .. } = *value else {
        unreachable!()
    };
    assert_eq!(fields["code"], BexExternalValue::String("23505".into()));
}
