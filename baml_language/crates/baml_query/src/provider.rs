use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use datafusion::arrow::array::{ArrayRef, Int64Builder, LargeBinaryBuilder, StringBuilder};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionContext;
use datafusion::logical_expr::async_udf::{AsyncScalarUDF, AsyncScalarUDFImpl};
use datafusion::logical_expr::{Expr, ScalarUDF, TableProviderFilterPushDown, TableType};
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::execution_plan::Boundedness;
use datafusion::physical_plan::memory::{LazyBatchGenerator, LazyMemoryExec};
use parking_lot::RwLock;
use rusqlite::Connection;

use crate::pushdown::SqlPredicate;
use crate::resident::{SqliteColumnSpec, SqliteResidentTableSpec, SqliteTableProvider};
use crate::{
    Hydrator, QueryContext, QueryError, RecursiveHydrator, Result, ValueId, ValueStore,
    register_builtin_functions,
};

const ARGS: usize = 10;
const RETURN: usize = 11;
const ERROR: usize = 12;
const DEFAULT_BATCH_SIZE: usize = 2048;

const FUNCTION_CALL_COLUMNS: [&str; 13] = [
    "id",
    "project_id",
    "process_id",
    "thread_id",
    "cct_id",
    "captured_ts",
    "name",
    "status",
    "metadata",
    "metrics",
    "args_value_id",
    "return_value_id",
    "error_value_id",
];

#[derive(Clone, Debug)]
pub struct FunctionCallsTableSpec {
    pub physical_name: String,
    columns: HashMap<String, String>,
}

impl FunctionCallsTableSpec {
    pub fn new(physical_name: impl Into<String>) -> Result<Self> {
        let physical_name = physical_name.into();
        validate_identifier(&physical_name)?;
        Ok(Self {
            physical_name,
            columns: FUNCTION_CALL_COLUMNS
                .into_iter()
                .map(|column| (column.to_owned(), column.to_owned()))
                .collect(),
        })
    }

    pub fn with_column(
        mut self,
        logical_name: impl Into<String>,
        physical_name: impl Into<String>,
    ) -> Result<Self> {
        let logical_name = logical_name.into();
        let physical_name = physical_name.into();
        if !FUNCTION_CALL_COLUMNS.contains(&logical_name.as_str()) {
            return Err(QueryError::Internal(format!(
                "unknown function_calls logical column: {logical_name}"
            )));
        }
        validate_identifier(&physical_name)?;
        self.columns.insert(logical_name, physical_name);
        Ok(self)
    }

    pub(crate) fn physical_column(&self, logical_name: &str) -> &str {
        self.columns
            .get(logical_name)
            .map(String::as_str)
            .expect("function_calls spec contains every logical column")
    }

    pub(crate) fn resident_mapping(&self) -> HashMap<String, String> {
        FUNCTION_CALL_COLUMNS[..10]
            .iter()
            .map(|logical_name| {
                (
                    (*logical_name).to_owned(),
                    quote_identifier(self.physical_column(logical_name)),
                )
            })
            .collect()
    }

    pub(crate) fn sqlite_table_spec(&self) -> Result<SqliteResidentTableSpec> {
        let columns = vec![
            SqliteColumnSpec::new("id", self.physical_column("id"), DataType::Utf8, false),
            SqliteColumnSpec::new(
                "project_id",
                self.physical_column("project_id"),
                DataType::Utf8,
                false,
            ),
            SqliteColumnSpec::new(
                "process_id",
                self.physical_column("process_id"),
                DataType::Utf8,
                true,
            ),
            SqliteColumnSpec::new(
                "thread_id",
                self.physical_column("thread_id"),
                DataType::Utf8,
                true,
            ),
            SqliteColumnSpec::new(
                "cct_id",
                self.physical_column("cct_id"),
                DataType::Utf8,
                true,
            ),
            SqliteColumnSpec::new(
                "captured_ts",
                self.physical_column("captured_ts"),
                DataType::Int64,
                true,
            ),
            SqliteColumnSpec::new("name", self.physical_column("name"), DataType::Utf8, false),
            SqliteColumnSpec::new(
                "status",
                self.physical_column("status"),
                DataType::Utf8,
                true,
            ),
            SqliteColumnSpec::new(
                "metadata",
                self.physical_column("metadata"),
                DataType::Utf8,
                true,
            ),
            SqliteColumnSpec::new(
                "metrics",
                self.physical_column("metrics"),
                DataType::Utf8,
                true,
            ),
            SqliteColumnSpec::hydrated_value("args", self.physical_column("args_value_id"), true),
            SqliteColumnSpec::hydrated_value(
                "return",
                self.physical_column("return_value_id"),
                true,
            ),
            SqliteColumnSpec::hydrated_value("error", self.physical_column("error_value_id"), true),
        ];
        SqliteResidentTableSpec::from_columns("function_calls", self.physical_name.clone(), columns)
    }
}

#[derive(Clone)]
pub struct SqliteFunctionCallsProvider {
    connection: Arc<Mutex<Connection>>,
    schema: SchemaRef,
    table_spec: FunctionCallsTableSpec,
    hydrator: Arc<dyn Hydrator>,
    query_context: QueryContext,
    batch_size: usize,
    resident_tables: Vec<SqliteResidentTableSpec>,
}

impl SqliteFunctionCallsProvider {
    pub fn open(
        path: impl AsRef<std::path::Path>,
        blob_root: impl AsRef<std::path::Path>,
        project_id: impl Into<Arc<str>>,
    ) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "query_only", true)?;
        Self::from_connection(connection, blob_root, project_id)
    }

    pub fn from_connection(
        connection: Connection,
        blob_root: impl AsRef<std::path::Path>,
        project_id: impl Into<Arc<str>>,
    ) -> Result<Self> {
        connection.pragma_update(None, "query_only", true)?;
        let store: Arc<dyn ValueStore> =
            Arc::new(crate::LocalBlobStore::new(blob_root.as_ref().to_path_buf()));
        Self::with_store(connection, store, project_id)
    }

    pub fn with_store(
        connection: Connection,
        store: Arc<dyn ValueStore>,
        project_id: impl Into<Arc<str>>,
    ) -> Result<Self> {
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            schema: logical_schema(),
            table_spec: FunctionCallsTableSpec::new("function_calls")?,
            hydrator: Arc::new(RecursiveHydrator::new(store)),
            query_context: QueryContext::new(project_id),
            batch_size: DEFAULT_BATCH_SIZE,
            resident_tables: Vec::new(),
        })
    }

    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    #[must_use]
    pub fn with_budgets(mut self, budgets: crate::QueryBudgets) -> Self {
        self.query_context = self.query_context.with_budgets(budgets);
        self
    }

    #[must_use]
    pub fn with_function_calls_table(mut self, table_spec: FunctionCallsTableSpec) -> Self {
        self.table_spec = table_spec;
        self
    }

    pub fn with_resident_table(mut self, spec: SqliteResidentTableSpec) -> Result<Self> {
        if spec.name == "function_calls" {
            return Err(QueryError::Internal(
                "function_calls is already registered".to_owned(),
            ));
        }
        self.resident_tables.push(spec);
        Ok(self)
    }

    pub fn with_table(self, spec: SqliteResidentTableSpec) -> Result<Self> {
        self.with_resident_table(spec)
    }

    pub fn with_standard_resident_tables(mut self) -> Result<Self> {
        for spec in crate::standard_resident_table_specs()? {
            self = self.with_resident_table(spec)?;
        }
        Ok(self)
    }

    pub fn query_context(&self) -> QueryContext {
        self.query_context.clone()
    }

    fn for_query(&self) -> Self {
        let mut provider = self.clone();
        provider.query_context = self.query_context.fresh();
        provider
    }

    pub fn metrics(&self) -> crate::QueryMetricsSnapshot {
        self.query_context.metrics_snapshot()
    }
}

impl fmt::Debug for SqliteFunctionCallsProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteFunctionCallsProvider")
            .field("schema", &self.schema)
            .field("table_spec", &self.table_spec)
            .field("project_id", &self.query_context.project_id)
            .field("batch_size", &self.batch_size)
            .field("resident_tables", &self.resident_tables)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl TableProvider for SqliteFunctionCallsProvider {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        let columns = self.table_spec.resident_mapping();
        Ok(filters
            .iter()
            .map(|filter| {
                if crate::pushdown::to_sql_for_columns(filter, &columns).is_some() {
                    TableProviderFilterPushDown::Exact
                } else {
                    TableProviderFilterPushDown::Unsupported
                }
            })
            .collect())
    }

    async fn scan(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let needs_values = projection
            .map(|projection| {
                [
                    projection.contains(&ARGS),
                    projection.contains(&RETURN),
                    projection.contains(&ERROR),
                ]
            })
            .unwrap_or([true; 3]);
        let generator = SqliteBatchGenerator {
            connection: Arc::clone(&self.connection),
            schema: self.schema.clone(),
            table_spec: self.table_spec.clone(),
            hydrator: Arc::clone(&self.hydrator),
            query_context: self.query_context.clone(),
            predicates: {
                let columns = self.table_spec.resident_mapping();
                filters
                    .iter()
                    .filter_map(|filter| crate::pushdown::to_sql_for_columns(filter, &columns))
                    .collect()
            },
            needs_values,
            batch_size: self.batch_size,
            offset: 0,
        };
        let exec = LazyMemoryExec::try_new(self.schema(), vec![Arc::new(RwLock::new(generator))])?
            .with_projection(projection.cloned());
        Ok(Arc::new(exec))
    }
}

struct SqliteBatchGenerator {
    connection: Arc<Mutex<Connection>>,
    schema: SchemaRef,
    table_spec: FunctionCallsTableSpec,
    hydrator: Arc<dyn Hydrator>,
    query_context: QueryContext,
    predicates: Vec<SqlPredicate>,
    needs_values: [bool; 3],
    batch_size: usize,
    offset: usize,
}

impl fmt::Debug for SqliteBatchGenerator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteBatchGenerator")
            .field("batch_size", &self.batch_size)
            .field("offset", &self.offset)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for SqliteBatchGenerator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let predicates = self
            .predicates
            .iter()
            .map(|predicate| predicate.sql.as_str())
            .collect::<Vec<_>>()
            .join(" AND ");
        write!(
            formatter,
            "SQLite function_calls batch_size={} offset={} predicates={}",
            self.batch_size, self.offset, predicates
        )
    }
}

impl LazyBatchGenerator for SqliteBatchGenerator {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn boundedness(&self) -> Boundedness {
        Boundedness::Bounded
    }

    fn generate_next_batch(&mut self) -> datafusion::error::Result<Option<RecordBatch>> {
        self.query_batch()
            .map_err(|error| DataFusionError::Execution(error.to_string()))
    }

    fn reset_state(&self) -> Arc<RwLock<dyn LazyBatchGenerator>> {
        Arc::new(RwLock::new(Self {
            connection: Arc::clone(&self.connection),
            schema: self.schema.clone(),
            table_spec: self.table_spec.clone(),
            hydrator: Arc::clone(&self.hydrator),
            query_context: self.query_context.clone(),
            predicates: self.predicates.clone(),
            needs_values: self.needs_values,
            batch_size: self.batch_size,
            offset: 0,
        }))
    }
}

impl SqliteBatchGenerator {
    fn query_batch(&mut self) -> Result<Option<RecordBatch>> {
        self.query_context.check_cancelled()?;
        let sqlite_started = std::time::Instant::now();
        let candidate_result = (|| -> Result<_> {
            let physical = |logical_name: &str| {
                quote_identifier(self.table_spec.physical_column(logical_name))
            };
            let (
                ids,
                project_ids,
                process_ids,
                thread_ids,
                cct_ids,
                captured_ts,
                names,
                statuses,
                metadata,
                metrics,
                args_ids,
                return_ids,
                error_ids,
            ) = {
                let connection = self
                    .connection
                    .lock()
                    .map_err(|_| QueryError::Internal("SQLite mutex poisoned".to_owned()))?;
                let mut sql = format!(
                    "SELECT {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}
                         FROM {} WHERE {} = ?",
                    physical("id"),
                    physical("project_id"),
                    physical("process_id"),
                    physical("thread_id"),
                    physical("cct_id"),
                    physical("captured_ts"),
                    physical("name"),
                    physical("status"),
                    physical("metadata"),
                    physical("metrics"),
                    physical("args_value_id"),
                    physical("return_value_id"),
                    physical("error_value_id"),
                    quote_identifier(&self.table_spec.physical_name),
                    physical("project_id"),
                );
                let mut params = vec![rusqlite::types::Value::Text(
                    self.query_context.project_id.to_string(),
                )];
                for predicate in &self.predicates {
                    sql.push_str(" AND ");
                    sql.push_str(&predicate.sql);
                    params.extend(predicate.params.iter().cloned());
                }
                sql.push_str(" ORDER BY rowid LIMIT ? OFFSET ?");
                params.push(rusqlite::types::Value::Integer(
                    i64::try_from(self.batch_size).map_err(|_| QueryError::ValueLimit)?,
                ));
                params.push(rusqlite::types::Value::Integer(
                    i64::try_from(self.offset).map_err(|_| QueryError::ValueLimit)?,
                ));
                let mut statement = connection.prepare(&sql)?;
                let mut rows = statement.query(rusqlite::params_from_iter(params.iter()))?;
                let mut ids = Vec::new();
                let mut project_ids = Vec::new();
                let mut process_ids = Vec::new();
                let mut thread_ids = Vec::new();
                let mut cct_ids = Vec::new();
                let mut captured_ts = Vec::new();
                let mut names = Vec::new();
                let mut statuses = Vec::new();
                let mut metadata = Vec::new();
                let mut metrics = Vec::new();
                let mut args_ids = Vec::new();
                let mut return_ids = Vec::new();
                let mut error_ids = Vec::new();
                while let Some(row) = rows.next()? {
                    ids.push(row.get::<_, String>(0)?);
                    project_ids.push(row.get::<_, String>(1)?);
                    process_ids.push(row.get::<_, Option<String>>(2)?);
                    thread_ids.push(row.get::<_, Option<String>>(3)?);
                    cct_ids.push(row.get::<_, Option<String>>(4)?);
                    captured_ts.push(row.get::<_, Option<i64>>(5)?);
                    names.push(row.get::<_, String>(6)?);
                    statuses.push(row.get::<_, Option<String>>(7)?);
                    metadata.push(row.get::<_, Option<String>>(8)?);
                    metrics.push(row.get::<_, Option<String>>(9)?);
                    args_ids.push(read_id(row, 10)?);
                    return_ids.push(read_id(row, 11)?);
                    error_ids.push(read_id(row, 12)?);
                }
                (
                    ids,
                    project_ids,
                    process_ids,
                    thread_ids,
                    cct_ids,
                    captured_ts,
                    names,
                    statuses,
                    metadata,
                    metrics,
                    args_ids,
                    return_ids,
                    error_ids,
                )
            };
            Ok((
                ids,
                project_ids,
                process_ids,
                thread_ids,
                cct_ids,
                captured_ts,
                names,
                statuses,
                metadata,
                metrics,
                args_ids,
                return_ids,
                error_ids,
            ))
        })();
        self.query_context
            .metrics
            .record_sqlite_duration(sqlite_started.elapsed());
        let (
            ids,
            project_ids,
            process_ids,
            thread_ids,
            cct_ids,
            captured_ts,
            names,
            statuses,
            metadata,
            metrics,
            args_ids,
            return_ids,
            error_ids,
        ) = candidate_result?;

        if ids.is_empty() {
            return Ok(None);
        }
        if self.offset.saturating_add(ids.len()) > self.query_context.budgets.max_candidate_rows {
            return Err(QueryError::ValueLimit);
        }
        self.offset += ids.len();
        self.query_context
            .metrics
            .batches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.query_context
            .metrics
            .input_rows
            .fetch_add(ids.len(), std::sync::atomic::Ordering::Relaxed);
        let args = self.hydrate_column(&args_ids, self.needs_values[0])?;
        let returns = self.hydrate_column(&return_ids, self.needs_values[1])?;
        let errors = self.hydrate_column(&error_ids, self.needs_values[2])?;
        let arrays: Vec<ArrayRef> = vec![
            strings(ids),
            strings(project_ids),
            optional_strings(process_ids),
            optional_strings(thread_ids),
            optional_strings(cct_ids),
            optional_ints(captured_ts),
            strings(names),
            optional_strings(statuses),
            optional_strings(metadata),
            optional_strings(metrics),
            binaries(args),
            binaries(returns),
            binaries(errors),
        ];
        Ok(Some(
            RecordBatch::try_new(self.schema.clone(), arrays)
                .map_err(|error| QueryError::Internal(error.to_string()))?,
        ))
    }

    fn hydrate_column(
        &self,
        ids: &[Option<ValueId>],
        needed: bool,
    ) -> Result<Vec<Option<Vec<u8>>>> {
        if !needed {
            return Ok(vec![None; ids.len()]);
        }
        let roots: Vec<ValueId> = ids.iter().flatten().copied().collect();
        if roots.is_empty() {
            return Ok(vec![None; ids.len()]);
        }
        let hydration_started = std::time::Instant::now();
        let values =
            futures::executor::block_on(self.hydrator.hydrate_many(&roots, &self.query_context))?;
        self.query_context
            .metrics
            .record_hydration_duration(hydration_started.elapsed());
        let serialization_started = std::time::Instant::now();
        let result = ids
            .iter()
            .map(|id| match id {
                None => Ok(None),
                Some(id) => {
                    let value = values.get(id).ok_or_else(|| QueryError::MissingValue {
                        value_id: id.to_string(),
                        path: std::path::PathBuf::new(),
                    })?;
                    Ok(Some(serde_json::to_vec(value)?))
                }
            })
            .collect();
        self.query_context
            .metrics
            .record_serialization_duration(serialization_started.elapsed());
        result
    }
}

pub struct QueryEngine {
    provider: SqliteFunctionCallsProvider,
    catalog: crate::QueryCatalog,
    udfs: parking_lot::Mutex<Vec<ScalarUDF>>,
    last_metrics: parking_lot::Mutex<crate::QueryMetricsSnapshot>,
}

impl QueryEngine {
    pub fn new(provider: SqliteFunctionCallsProvider) -> Result<Self> {
        let mut catalog = crate::QueryCatalog::new();
        catalog.register_sqlite_table(&provider.table_spec.sqlite_table_spec()?)?;
        for spec in &provider.resident_tables {
            catalog.register_sqlite_table(spec)?;
        }
        Ok(Self {
            provider,
            catalog,
            udfs: parking_lot::Mutex::new(Vec::new()),
            last_metrics: parking_lot::Mutex::new(crate::QueryMetricsSnapshot::default()),
        })
    }

    fn new_context(&self) -> Result<(SessionContext, QueryContext)> {
        let provider = self.provider.for_query();
        let query_context = provider.query_context();
        let context = SessionContext::new();
        context.register_table(
            "function_calls",
            Arc::new(SqliteTableProvider::new(
                Arc::clone(&provider.connection),
                provider.table_spec.sqlite_table_spec()?,
                Arc::clone(&provider.hydrator),
                query_context.clone(),
                provider.batch_size,
            )),
        )?;
        for spec in &self.provider.resident_tables {
            context.register_table(
                spec.name.clone(),
                Arc::new(SqliteTableProvider::new(
                    Arc::clone(&self.provider.connection),
                    spec.clone(),
                    Arc::clone(&self.provider.hydrator),
                    query_context.clone(),
                    self.provider.batch_size,
                )),
            )?;
        }
        register_builtin_functions(&context);
        for udf in self.udfs.lock().iter().cloned() {
            context.register_udf(udf);
        }
        Ok((context, query_context))
    }

    pub fn with_relationship(
        mut self,
        relationship: crate::RelationshipDefinition,
    ) -> Result<Self> {
        self.catalog.register_relationship(relationship)?;
        Ok(self)
    }

    #[must_use]
    pub fn catalog(&self) -> &crate::QueryCatalog {
        &self.catalog
    }

    /// Registers an allowlisted `DataFusion` scalar UDF for subsequent queries.
    /// Async UDFs can be wrapped with `DataFusion`'s `AsyncScalarUDF` adapter.
    pub fn register_udf(&self, udf: ScalarUDF) {
        self.udfs.lock().push(udf);
    }

    /// Registers a batch-oriented asynchronous UDF implementation.
    pub fn register_async_udf(&self, udf: Arc<dyn AsyncScalarUDFImpl>) {
        self.register_udf(AsyncScalarUDF::new(udf).into_scalar_udf());
    }

    pub fn query_context(&self) -> QueryContext {
        self.provider.query_context()
    }

    pub fn metrics(&self) -> crate::QueryMetricsSnapshot {
        self.last_metrics.lock().clone()
    }

    pub fn cancel(&self) {
        self.provider.query_context().cancellation.cancel();
    }

    pub async fn execute(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        validate_read_only(sql)?;
        let (context, query_context) = self.new_context()?;
        let started = std::time::Instant::now();
        let result = match context.sql(sql).await {
            Ok(dataframe) => dataframe.collect().await,
            Err(error) => Err(error),
        };
        query_context
            .metrics
            .record_query_duration(started.elapsed());
        let batches = result?;
        query_context.metrics.output_rows.fetch_add(
            batches.iter().map(RecordBatch::num_rows).sum(),
            std::sync::atomic::Ordering::Relaxed,
        );
        *self.last_metrics.lock() = query_context.metrics_snapshot();
        Ok(batches)
    }

    pub async fn explain(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        validate_read_only(sql)?;
        let (context, query_context) = self.new_context()?;
        let started = std::time::Instant::now();
        let result = match context.sql(&format!("EXPLAIN {sql}")).await {
            Ok(dataframe) => dataframe.collect().await,
            Err(error) => Err(error),
        };
        query_context
            .metrics
            .record_query_duration(started.elapsed());
        let batches = result?;
        query_context.metrics.output_rows.fetch_add(
            batches.iter().map(RecordBatch::num_rows).sum(),
            std::sync::atomic::Ordering::Relaxed,
        );
        *self.last_metrics.lock() = query_context.metrics_snapshot();
        Ok(batches)
    }
}

fn logical_schema() -> SchemaRef {
    let baml_metadata = HashMap::from([
        (
            "ARROW:extension:name".to_owned(),
            "boundary.baml_value".to_owned(),
        ),
        ("ARROW:extension:metadata".to_owned(), "v1".to_owned()),
    ]);
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("project_id", DataType::Utf8, false),
        Field::new("process_id", DataType::Utf8, true),
        Field::new("thread_id", DataType::Utf8, true),
        Field::new("cct_id", DataType::Utf8, true),
        Field::new("captured_ts", DataType::Int64, true),
        Field::new("name", DataType::Utf8, false),
        Field::new("status", DataType::Utf8, true),
        Field::new("metadata", DataType::Utf8, true),
        Field::new("metrics", DataType::Utf8, true),
        Field::new("args", DataType::LargeBinary, true).with_metadata(baml_metadata.clone()),
        Field::new("return", DataType::LargeBinary, true).with_metadata(baml_metadata.clone()),
        Field::new("error", DataType::LargeBinary, true).with_metadata(baml_metadata),
    ]))
}

fn read_id(row: &rusqlite::Row<'_>, index: usize) -> Result<Option<ValueId>> {
    let bytes: Option<Vec<u8>> = row.get(index)?;
    match bytes {
        None => Ok(None),
        Some(bytes) if bytes.len() == ValueId::LEN => {
            Ok(Some(ValueId::from_bytes(bytes.try_into().map_err(
                |_| QueryError::InvalidValueId("wrong byte length".to_owned()),
            )?)))
        }
        Some(bytes) => Err(QueryError::InvalidValueId(format!(
            "expected 32 bytes, got {}",
            bytes.len()
        ))),
    }
}

fn strings(values: Vec<String>) -> ArrayRef {
    let mut builder = StringBuilder::new();
    for value in values {
        builder.append_value(value);
    }
    Arc::new(builder.finish())
}

fn optional_strings(values: Vec<Option<String>>) -> ArrayRef {
    let mut builder = StringBuilder::new();
    for value in values {
        match value {
            Some(value) => builder.append_value(value),
            None => builder.append_null(),
        }
    }
    Arc::new(builder.finish())
}

fn optional_ints(values: Vec<Option<i64>>) -> ArrayRef {
    let mut builder = Int64Builder::new();
    for value in values {
        match value {
            Some(value) => builder.append_value(value),
            None => builder.append_null(),
        }
    }
    Arc::new(builder.finish())
}

fn binaries(values: Vec<Option<Vec<u8>>>) -> ArrayRef {
    let mut builder = LargeBinaryBuilder::new();
    for value in values {
        match value {
            Some(value) => builder.append_value(value),
            None => builder.append_null(),
        }
    }
    Arc::new(builder.finish())
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn validate_identifier(identifier: &str) -> Result<()> {
    if identifier.is_empty()
        || !identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(QueryError::Internal(format!(
            "invalid SQLite identifier: {identifier}"
        )));
    }
    Ok(())
}

fn validate_read_only(sql: &str) -> Result<()> {
    let statements = sql
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .collect::<Vec<_>>();
    if statements.len() != 1 {
        return Err(QueryError::NotReadOnly(
            "multiple statements are not allowed".to_owned(),
        ));
    }
    let normalized = statements[0].to_ascii_lowercase();
    if !(normalized.starts_with("select ")
        || normalized == "select"
        || normalized.starts_with("with ")
        || normalized.starts_with("explain "))
    {
        return Err(QueryError::NotReadOnly(statements[0].to_owned()));
    }
    Ok(())
}
