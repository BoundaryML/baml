//! SQL over resident `SQLite` columns with transparent value-file hydration.
//!
//! The crate deliberately keeps the physical representation separate from the
//! SQL-facing schema. A query sees `args`, `return`, and `error`; `SQLite` stores
//! nullable content IDs and the value store resolves those IDs on demand.

mod catalog;
mod context;
mod error;
mod functions;
mod hydrator;
mod provider;
mod pushdown;
mod resident;
mod store;
mod value_id;

pub use catalog::{QueryCatalog, RelationshipCardinality, RelationshipDefinition, TableDefinition};
pub use context::{QueryBudgets, QueryContext, QueryMetricsSnapshot};
pub use error::{QueryError, Result};
pub use functions::register_builtin_functions;
pub use hydrator::{HydratedValue, Hydrator, RecursiveHydrator, ValueReference};
pub use provider::{FunctionCallsTableSpec, QueryEngine, SqliteFunctionCallsProvider};
pub use resident::SqliteColumnSpec;
pub use resident::{
    SqliteResidentTableSpec, SqliteTableProvider, SqliteTableSpec, standard_resident_table_specs,
};
pub use store::{LocalBlobStore, ValueStore};
pub use value_id::ValueId;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datafusion::arrow::array::{
        Array, BooleanBuilder, Int64Array, LargeBinaryArray, StringArray, UInt64Array,
    };
    use datafusion::arrow::datatypes::DataType;
    use datafusion::error::DataFusionError;
    use datafusion::logical_expr::async_udf::AsyncScalarUDFImpl;
    use datafusion::logical_expr::{
        ColumnarValue, ScalarFunctionArgs, ScalarUDFImpl, Signature, Volatility,
    };
    use rusqlite::Connection;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn create_database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE function_calls (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                process_id TEXT,
                thread_id TEXT,
                cct_id TEXT,
                captured_ts INTEGER,
                name TEXT NOT NULL,
                status TEXT,
                metadata TEXT,
                metrics TEXT,
                args_value_id BLOB,
                return_value_id BLOB,
                error_value_id BLOB
            );",
            )
            .unwrap();
        connection
    }

    fn insert_call(connection: &Connection, id: &str, project_id: &str, args_id: Option<ValueId>) {
        connection.execute(
            "INSERT INTO function_calls (id, project_id, name, args_value_id) VALUES (?1, ?2, 'send_email', ?3)",
            rusqlite::params![id, project_id, args_id.map(|id| id.as_bytes().to_vec())],
        ).unwrap();
    }

    #[derive(Debug, Eq, Hash, PartialEq)]
    struct IsSendEmail {
        signature: Signature,
    }

    impl ScalarUDFImpl for IsSendEmail {
        fn name(&self) -> &'static str {
            "is_send_email"
        }

        fn signature(&self) -> &Signature {
            &self.signature
        }

        fn return_type(&self, _arg_types: &[DataType]) -> datafusion::error::Result<DataType> {
            Ok(DataType::Boolean)
        }

        fn invoke_with_args(
            &self,
            _args: ScalarFunctionArgs,
        ) -> datafusion::error::Result<ColumnarValue> {
            Err(DataFusionError::Execution(
                "is_send_email must execute asynchronously".to_owned(),
            ))
        }
    }

    #[async_trait::async_trait]
    impl AsyncScalarUDFImpl for IsSendEmail {
        async fn invoke_async_with_args(
            &self,
            function_args: ScalarFunctionArgs,
        ) -> datafusion::error::Result<ColumnarValue> {
            let values = match function_args.args.first() {
                Some(ColumnarValue::Array(array)) => array
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| {
                    DataFusionError::Execution("expected string input".to_owned())
                })?,
                _ => {
                    return Err(DataFusionError::Execution(
                        "expected array input".to_owned(),
                    ));
                }
            };
            let mut output = BooleanBuilder::new();
            for row in 0..values.len() {
                output.append_value(!values.is_null(row) && values.value(row) == "send_email");
            }
            Ok(ColumnarValue::Array(Arc::new(output.finish())))
        }
    }

    fn engine_for(connection: Connection, temp_dir: &TempDir) -> QueryEngine {
        QueryEngine::new(
            SqliteFunctionCallsProvider::from_connection(
                connection,
                temp_dir.path(),
                Arc::<str>::from("p1"),
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn hydrates_nested_value_references() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let child_bytes = serde_json::to_vec(&json!("hello")).unwrap();
        let child_id = ValueId::from_content(&child_bytes);
        store.put(child_id, &child_bytes).await.unwrap();

        let root_value = json!([{"$value_ref": child_id.to_hex()}]);
        let root_bytes = serde_json::to_vec(&root_value).unwrap();
        let root_id = ValueId::from_content(&root_bytes);
        store.put(root_id, &root_bytes).await.unwrap();

        let connection = create_database();
        insert_call(&connection, "call-1", "p1", Some(root_id));
        let engine = engine_for(connection, &temp_dir);
        let batches = engine
            .execute("SELECT id FROM function_calls WHERE contains(value_at(args, 0), 'hello')")
            .await
            .unwrap();

        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(ids.value(0), "call-1");
    }

    #[tokio::test]
    async fn final_limit_is_applied_after_hydrated_filter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let miss_bytes = serde_json::to_vec(&json!(["nope"])).unwrap();
        let miss_id = ValueId::from_content(&miss_bytes);
        store.put(miss_id, &miss_bytes).await.unwrap();
        let hit_bytes = serde_json::to_vec(&json!(["hi"])).unwrap();
        let hit_id = ValueId::from_content(&hit_bytes);
        store.put(hit_id, &hit_bytes).await.unwrap();

        let connection = create_database();
        for index in 0..500 {
            insert_call(&connection, &format!("miss-{index}"), "p1", Some(miss_id));
        }
        for index in 0..100 {
            insert_call(&connection, &format!("hit-{index}"), "p1", Some(hit_id));
        }
        let engine = engine_for(connection, &temp_dir);
        let batches = engine
            .execute(
                "SELECT id FROM function_calls WHERE contains(value_at(args, 0), 'hi') LIMIT 100",
            )
            .await
            .unwrap();

        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(ids.len(), 100);
        assert!(ids.value(0).starts_with("hit-"));
    }

    #[tokio::test]
    async fn selecting_resident_columns_does_not_require_blob_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", None);
        let engine = engine_for(connection, &temp_dir);
        let batches = engine
            .execute("SELECT id, name FROM function_calls")
            .await
            .unwrap();

        assert_eq!(batches[0].num_rows(), 1);
        assert_eq!(batches[0].schema().field(0).name(), "id");
    }

    #[tokio::test]
    async fn projection_prunes_unrequested_hydrated_columns() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let args_bytes = serde_json::to_vec(&json!(["args"])).unwrap();
        let args_id = ValueId::from_content(&args_bytes);
        store.put(args_id, &args_bytes).await.unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", Some(args_id));
        let engine = engine_for(connection, &temp_dir);

        let batches = engine
            .execute("SELECT args FROM function_calls")
            .await
            .unwrap();
        let values = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<LargeBinaryArray>()
            .unwrap();
        assert_eq!(values.value(0), args_bytes.as_slice());
    }

    #[tokio::test]
    async fn hydrated_columns_are_available_as_json_bytes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let bytes = serde_json::to_vec(&json!({"subject": "hello"})).unwrap();
        let id = ValueId::from_content(&bytes);
        store.put(id, &bytes).await.unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", Some(id));
        let engine = engine_for(connection, &temp_dir);
        let batches = engine
            .execute("SELECT args FROM function_calls")
            .await
            .unwrap();

        let values = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<LargeBinaryArray>()
            .unwrap();
        assert_eq!(values.value(0), bytes.as_slice());
    }

    #[tokio::test]
    async fn value_field_and_value_string_compose_in_sql() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let bytes = serde_json::to_vec(&json!({"subject": "hello"})).unwrap();
        let id = ValueId::from_content(&bytes);
        store.put(id, &bytes).await.unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", Some(id));
        let engine = engine_for(connection, &temp_dir);

        let batches = engine
            .execute("SELECT id FROM function_calls WHERE value_string(value_field(args, 'subject')) = 'hello'")
            .await
            .unwrap();
        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(ids.value(0), "call-1");
    }

    #[tokio::test]
    async fn ctes_keep_resident_and_hydrated_filters_correct() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let bytes = serde_json::to_vec(&json!(["hello"])).unwrap();
        let id = ValueId::from_content(&bytes);
        store.put(id, &bytes).await.unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", Some(id));
        let engine = engine_for(connection, &temp_dir);

        let batches = engine
            .execute(
                "WITH recent AS (SELECT * FROM function_calls WHERE name = 'send_email')
                 SELECT id FROM recent
                 WHERE contains(value_at(args, 0), 'hello')",
            )
            .await
            .unwrap();
        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(ids.value(0), "call-1");
    }

    #[tokio::test]
    async fn mixed_or_predicates_are_not_partially_pushed() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let hi_bytes = serde_json::to_vec(&json!(["hi"])).unwrap();
        let hi_id = ValueId::from_content(&hi_bytes);
        store.put(hi_id, &hi_bytes).await.unwrap();
        let no_bytes = serde_json::to_vec(&json!(["no"])).unwrap();
        let no_id = ValueId::from_content(&no_bytes);
        store.put(no_id, &no_bytes).await.unwrap();
        let connection = create_database();
        connection
            .execute(
                "INSERT INTO function_calls (id, project_id, name, args_value_id) VALUES ('call-other', 'p1', 'other', ?1)",
                rusqlite::params![hi_id.as_bytes().to_vec()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO function_calls (id, project_id, name, args_value_id) VALUES ('call-send', 'p1', 'send_email', ?1)",
                rusqlite::params![no_id.as_bytes().to_vec()],
            )
            .unwrap();
        let engine = engine_for(connection, &temp_dir);

        let batches = engine
            .execute("SELECT id FROM function_calls WHERE name = 'send_email' OR contains(value_at(args, 0), 'hi') ORDER BY id")
            .await
            .unwrap();
        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids.value(0), "call-other");
        assert_eq!(ids.value(1), "call-send");
    }

    #[tokio::test]
    async fn resident_filters_are_pushed_before_hydration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        let missing_id = ValueId::from_content(b"missing");
        connection
            .execute(
                "INSERT INTO function_calls (id, project_id, name, args_value_id) VALUES ('call-1', 'p1', 'other', ?1)",
                rusqlite::params![missing_id.as_bytes().to_vec()],
            )
            .unwrap();
        let engine = engine_for(connection, &temp_dir);

        let batches = engine
            .execute("SELECT args FROM function_calls WHERE name = 'send_email'")
            .await
            .unwrap();
        assert_eq!(
            batches
                .iter()
                .map(datafusion::arrow::record_batch::RecordBatch::num_rows)
                .sum::<usize>(),
            0
        );
    }

    #[tokio::test]
    async fn async_udfs_can_be_registered_and_used_in_filters() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", None);
        let engine = engine_for(connection, &temp_dir);
        engine.register_async_udf(Arc::new(IsSendEmail {
            signature: Signature::exact(vec![DataType::Utf8], Volatility::Immutable),
        }));

        let batches = engine
            .execute("SELECT id FROM function_calls WHERE is_send_email(name)")
            .await
            .unwrap();
        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(ids.value(0), "call-1");
    }

    #[tokio::test]
    async fn cancellation_stops_lazy_scan() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", None);
        let engine = engine_for(connection, &temp_dir);
        engine.cancel();

        let error = engine
            .execute("SELECT id FROM function_calls")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn standard_resident_tables_support_project_scoped_joins() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        connection
            .execute_batch(
                "CREATE TABLE processes (id TEXT PRIMARY KEY, project_id TEXT NOT NULL, name TEXT);
                 CREATE TABLE threads (id TEXT PRIMARY KEY, project_id TEXT NOT NULL, process_id TEXT, name TEXT);
                 CREATE TABLE ccts (id TEXT PRIMARY KEY, project_id TEXT NOT NULL, thread_id TEXT, name TEXT);
                 INSERT INTO processes VALUES ('process-1', 'p1', 'worker');
                 INSERT INTO threads VALUES ('thread-1', 'p1', 'process-1', 'thread');
                 INSERT INTO ccts VALUES ('cct-1', 'p1', 'thread-1', 'root');",
            )
            .unwrap();
        insert_call(&connection, "call-1", "p1", None);
        connection
            .execute(
                "UPDATE function_calls SET process_id = 'process-1', thread_id = 'thread-1', cct_id = 'cct-1'
                 WHERE id = 'call-1'",
                [],
            )
            .unwrap();
        let provider = SqliteFunctionCallsProvider::from_connection(
            connection,
            temp_dir.path(),
            Arc::<str>::from("p1"),
        )
        .unwrap()
        .with_standard_resident_tables()
        .unwrap();
        let engine = QueryEngine::new(provider).unwrap();

        let batches = engine
            .execute(
                "SELECT f.id, p.name, t.name, c.name
                 FROM function_calls f
                 JOIN processes p ON p.id = f.process_id
                 JOIN threads t ON t.id = f.thread_id
                 JOIN ccts c ON c.id = f.cct_id
                 WHERE f.name = 'send_email'",
            )
            .await
            .unwrap();
        assert_eq!(batches[0].num_rows(), 1);
        let id = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let process_name = batches[0]
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(id.value(0), "call-1");
        assert_eq!(process_name.value(0), "worker");
    }

    #[tokio::test]
    async fn explain_exposes_the_sqlite_scan() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", None);
        let engine = engine_for(connection, &temp_dir);
        let batches = engine
            .explain("SELECT id FROM function_calls WHERE name = 'send_email'")
            .await
            .unwrap();
        assert!(!batches.is_empty());
        let plan = batches
            .iter()
            .flat_map(|batch| {
                batch
                    .columns()
                    .iter()
                    .filter_map(|column| column.as_any().downcast_ref::<StringArray>())
                    .flat_map(|column| (0..column.len()).map(|row| column.value(row).to_owned()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(plan.contains("SQLite function_calls"));
    }

    #[tokio::test]
    async fn candidate_row_budget_is_enforced() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", None);
        let provider = SqliteFunctionCallsProvider::from_connection(
            connection,
            temp_dir.path(),
            Arc::<str>::from("p1"),
        )
        .unwrap()
        .with_budgets(QueryBudgets {
            max_candidate_rows: 0,
            ..QueryBudgets::default()
        });
        let engine = QueryEngine::new(provider).unwrap();
        let error = engine
            .execute("SELECT id FROM function_calls")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("limit"));
    }

    #[tokio::test]
    async fn metrics_capture_query_work_and_phase_timings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", None);
        let engine = engine_for(connection, &temp_dir);

        engine
            .execute("SELECT id FROM function_calls WHERE name = 'send_email'")
            .await
            .unwrap();

        let metrics = engine.metrics();
        assert_eq!(metrics.input_rows, 1);
        assert_eq!(metrics.output_rows, 1);
        assert_eq!(metrics.batches, 1);
        assert!(metrics.query_duration > std::time::Duration::ZERO);
        assert!(metrics.sqlite_duration > std::time::Duration::ZERO);
    }

    #[tokio::test]
    async fn externally_defined_physical_layouts_are_mapped_to_logical_sql() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let args_bytes = serde_json::to_vec(&json!(["hi"])).unwrap();
        let args_id = ValueId::from_content(&args_bytes);
        store.put(args_id, &args_bytes).await.unwrap();

        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE trace_rows (
                    call_uuid TEXT PRIMARY KEY,
                    tenant_key TEXT NOT NULL,
                    proc_ref TEXT,
                    thread_ref TEXT,
                    cct_ref TEXT,
                    event_time INTEGER,
                    fn_name TEXT NOT NULL,
                    state TEXT,
                    meta_json TEXT,
                    metric_json TEXT,
                    arg_cid BLOB,
                    ret_cid BLOB,
                    err_cid BLOB
                );
                 CREATE TABLE process_catalog (
                    process_key TEXT PRIMARY KEY,
                    tenant_key TEXT NOT NULL,
                    program_name TEXT
                 );
                 INSERT INTO process_catalog VALUES ('process-1', 'p1', 'worker');",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO trace_rows
                 (call_uuid, tenant_key, proc_ref, fn_name, arg_cid)
                 VALUES ('call-1', 'p1', 'process-1', 'send_email', ?1)",
                rusqlite::params![args_id.as_bytes().to_vec()],
            )
            .unwrap();

        let function_calls = FunctionCallsTableSpec::new("trace_rows")
            .unwrap()
            .with_column("id", "call_uuid")
            .unwrap()
            .with_column("project_id", "tenant_key")
            .unwrap()
            .with_column("process_id", "proc_ref")
            .unwrap()
            .with_column("thread_id", "thread_ref")
            .unwrap()
            .with_column("cct_id", "cct_ref")
            .unwrap()
            .with_column("captured_ts", "event_time")
            .unwrap()
            .with_column("name", "fn_name")
            .unwrap()
            .with_column("status", "state")
            .unwrap()
            .with_column("metadata", "meta_json")
            .unwrap()
            .with_column("metrics", "metric_json")
            .unwrap()
            .with_column("args_value_id", "arg_cid")
            .unwrap()
            .with_column("return_value_id", "ret_cid")
            .unwrap()
            .with_column("error_value_id", "err_cid")
            .unwrap();
        let process_table = SqliteResidentTableSpec::from_columns(
            "processes",
            "process_catalog",
            vec![
                SqliteColumnSpec::new("id", "process_key", DataType::Utf8, false),
                SqliteColumnSpec::new("project_id", "tenant_key", DataType::Utf8, false),
                SqliteColumnSpec::new("name", "program_name", DataType::Utf8, true),
            ],
        )
        .unwrap();
        let provider = SqliteFunctionCallsProvider::from_connection(
            connection,
            temp_dir.path(),
            Arc::<str>::from("p1"),
        )
        .unwrap()
        .with_function_calls_table(function_calls)
        .with_resident_table(process_table)
        .unwrap();
        let engine = QueryEngine::new(provider).unwrap();

        let batches = engine
            .execute(
                "SELECT f.id, p.name
                 FROM function_calls f
                 JOIN processes p ON p.id = f.process_id
                 WHERE f.name = 'send_email'
                   AND contains(value_at(f.args, 0), 'hi')",
            )
            .await
            .unwrap();
        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let names = batches[0]
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(ids.value(0), "call-1");
        assert_eq!(names.value(0), "worker");
    }

    #[tokio::test]
    async fn arbitrary_table_specs_support_hydration_and_declared_relationships() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let value_bytes = serde_json::to_vec(&json!("hello")).unwrap();
        let value_id = ValueId::from_content(&value_bytes);
        store.put(value_id, &value_bytes).await.unwrap();

        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE event_log (
                    event_key INTEGER PRIMARY KEY,
                    tenant_key TEXT NOT NULL,
                    process_ref INTEGER,
                    payload_cid BLOB
                 );
                 CREATE TABLE process_catalog (
                    process_key INTEGER PRIMARY KEY,
                    tenant_key TEXT NOT NULL,
                    program_name TEXT
                 );
                 INSERT INTO process_catalog VALUES (7, 'p1', 'worker');
                 INSERT INTO event_log VALUES (42, 'p1', 7, NULL);",
            )
            .unwrap();
        connection
            .execute(
                "UPDATE event_log SET payload_cid = ?1 WHERE event_key = 42",
                rusqlite::params![value_id.as_bytes().to_vec()],
            )
            .unwrap();

        let events = SqliteTableSpec::from_columns(
            "events",
            "event_log",
            vec![
                SqliteColumnSpec::new("event_id", "event_key", DataType::UInt64, false),
                SqliteColumnSpec::new("project_id", "tenant_key", DataType::Utf8, false),
                SqliteColumnSpec::new("process_ref", "process_ref", DataType::UInt64, true),
                SqliteColumnSpec::hydrated_value("payload", "payload_cid", true),
            ],
        )
        .unwrap();
        let processes = SqliteTableSpec::from_columns(
            "processes",
            "process_catalog",
            vec![
                SqliteColumnSpec::new("id", "process_key", DataType::UInt64, false),
                SqliteColumnSpec::new("project_id", "tenant_key", DataType::Utf8, false),
                SqliteColumnSpec::new("name", "program_name", DataType::Utf8, true),
            ],
        )
        .unwrap();
        let provider = SqliteFunctionCallsProvider::from_connection(
            connection,
            temp_dir.path(),
            Arc::<str>::from("p1"),
        )
        .unwrap()
        .with_table(events)
        .unwrap()
        .with_table(processes)
        .unwrap();
        let engine = QueryEngine::new(provider)
            .unwrap()
            .with_relationship(
                RelationshipDefinition::many_to_one("events", "process_ref", "processes", "id")
                    .project_scoped(),
            )
            .unwrap();

        assert_eq!(engine.catalog().relationships().len(), 1);
        let batches = engine
            .execute(
                "SELECT e.event_id
                 FROM events e
                 JOIN processes p ON p.id = e.process_ref
                 WHERE p.name = 'worker' AND value_string(e.payload) = 'hello'",
            )
            .await
            .unwrap();
        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        assert_eq!(ids.value(0), 42);
    }

    #[tokio::test]
    async fn resident_aggregations_work_without_hydration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = create_database();
        connection
            .execute(
                "INSERT INTO function_calls (id, project_id, name, status) VALUES ('call-1', 'p1', 'send_email', 'ok')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO function_calls (id, project_id, name, status) VALUES ('call-2', 'p1', 'send_email', 'ok')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO function_calls (id, project_id, name, status) VALUES ('call-3', 'p1', 'refund', 'error')",
                [],
            )
            .unwrap();
        let engine = engine_for(connection, &temp_dir);

        let batches = engine
            .execute(
                "SELECT name, COUNT(*) AS count
                 FROM function_calls
                 GROUP BY name
                 ORDER BY name",
            )
            .await
            .unwrap();
        assert_eq!(batches.len(), 1);
        let names = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let counts = batches[0]
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(names.value(0), "refund");
        assert_eq!(names.value(1), "send_email");
        assert_eq!(counts.values(), &[1, 2]);
        assert_eq!(engine.metrics().blob_requests, 0);

        let distinct_batches = engine
            .execute(
                "SELECT COUNT(DISTINCT name) AS distinct_names
                 FROM function_calls
                 HAVING COUNT(*) = 3",
            )
            .await
            .unwrap();
        let distinct_names = distinct_batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(distinct_names.value(0), 2);
    }

    #[tokio::test]
    async fn hydrated_scalar_aggregations_work() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let make_value = |category: &str| {
            let bytes = serde_json::to_vec(&json!({"category": category})).unwrap();
            let id = ValueId::from_content(&bytes);
            (id, bytes)
        };
        let (alpha_id, alpha_bytes) = make_value("alpha");
        let (beta_id, beta_bytes) = make_value("beta");
        store.put(alpha_id, &alpha_bytes).await.unwrap();
        store.put(beta_id, &beta_bytes).await.unwrap();
        let connection = create_database();
        insert_call(&connection, "call-1", "p1", Some(alpha_id));
        insert_call(&connection, "call-2", "p1", Some(alpha_id));
        insert_call(&connection, "call-3", "p1", Some(beta_id));
        let engine = engine_for(connection, &temp_dir);

        let batches = engine
            .execute(
                "SELECT value_string(value_field(args, 'category')) AS category, COUNT(*) AS count
                 FROM function_calls
                 GROUP BY category
                 ORDER BY category",
            )
            .await
            .unwrap();
        let categories = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let counts = batches[0]
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(categories.value(0), "alpha");
        assert_eq!(categories.value(1), "beta");
        assert_eq!(counts.values(), &[2, 1]);
        assert_eq!(engine.metrics().blob_requests, 2);
    }

    #[tokio::test]
    async fn aggregation_after_hydrated_filter_counts_all_survivors() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(temp_dir.path());
        let make_value = |value: &str| {
            let bytes = serde_json::to_vec(&json!([value])).unwrap();
            let id = ValueId::from_content(&bytes);
            (id, bytes)
        };
        let (hit_id, hit_bytes) = make_value("hit");
        let (miss_id, miss_bytes) = make_value("miss");
        store.put(hit_id, &hit_bytes).await.unwrap();
        store.put(miss_id, &miss_bytes).await.unwrap();
        let connection = create_database();
        for index in 0..5 {
            insert_call(&connection, &format!("hit-{index}"), "p1", Some(hit_id));
        }
        for index in 0..7 {
            insert_call(&connection, &format!("miss-{index}"), "p1", Some(miss_id));
        }
        let engine = engine_for(connection, &temp_dir);

        let batches = engine
            .execute(
                "SELECT COUNT(*) AS count
                 FROM function_calls
                 WHERE contains(value_at(args, 0), 'hit')",
            )
            .await
            .unwrap();
        let counts = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(counts.value(0), 5);
        assert_eq!(engine.metrics().input_rows, 12);
        assert_eq!(engine.metrics().output_rows, 1);
    }
}
