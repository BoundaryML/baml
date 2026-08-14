use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use datafusion::arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Builder, LargeBinaryBuilder, LargeStringBuilder,
    StringBuilder, UInt64Builder,
};
use datafusion::arrow::datatypes::{DataType, Field, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown, TableType};
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::execution_plan::Boundedness;
use datafusion::physical_plan::memory::{LazyBatchGenerator, LazyMemoryExec};
use parking_lot::RwLock;
use rusqlite::types::Value;
use rusqlite::{Connection, Row};

use crate::pushdown::SqlPredicate;
use crate::{Hydrator, QueryContext, QueryError, Result, ValueId};

#[derive(Clone, Debug)]
pub struct SqliteColumnSpec {
    pub logical_name: String,
    pub physical_name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub hydrated: bool,
}

impl SqliteColumnSpec {
    pub fn new(
        logical_name: impl Into<String>,
        physical_name: impl Into<String>,
        data_type: DataType,
        nullable: bool,
    ) -> Self {
        Self {
            logical_name: logical_name.into(),
            physical_name: physical_name.into(),
            data_type,
            nullable,
            hydrated: false,
        }
    }

    #[must_use]
    pub fn hydrated_value(
        logical_name: impl Into<String>,
        physical_name: impl Into<String>,
        nullable: bool,
    ) -> Self {
        Self {
            logical_name: logical_name.into(),
            physical_name: physical_name.into(),
            data_type: DataType::LargeBinary,
            nullable,
            hydrated: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SqliteResidentTableSpec {
    pub name: String,
    pub physical_name: String,
    pub schema: SchemaRef,
    pub columns: Vec<SqliteColumnSpec>,
}

/// Alias emphasizing that this specification can also contain hydrated value
/// columns; `SqliteResidentTableSpec` is retained for compatibility.
pub type SqliteTableSpec = SqliteResidentTableSpec;

impl SqliteResidentTableSpec {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        name: impl Into<String>,
        physical_name: impl Into<String>,
        schema: SchemaRef,
    ) -> Result<Self> {
        let name = name.into();
        let physical_name = physical_name.into();
        validate_identifier(&name)?;
        validate_identifier(&physical_name)?;
        let columns = schema
            .fields()
            .iter()
            .map(|field| {
                SqliteColumnSpec::new(
                    field.name(),
                    field.name(),
                    field.data_type().clone(),
                    field.is_nullable(),
                )
            })
            .collect();
        Self::from_columns(name, physical_name, columns)
    }

    pub fn from_columns(
        name: impl Into<String>,
        physical_name: impl Into<String>,
        columns: Vec<SqliteColumnSpec>,
    ) -> Result<Self> {
        let name = name.into();
        let physical_name = physical_name.into();
        validate_identifier(&name)?;
        validate_identifier(&physical_name)?;
        if columns
            .iter()
            .all(|column| column.logical_name != "project_id")
        {
            return Err(QueryError::Internal(format!(
                "resident table {name} must contain project_id"
            )));
        }
        let mut logical_names = HashMap::new();
        let fields = columns
            .iter()
            .map(|column| {
                validate_identifier(&column.logical_name)?;
                validate_identifier(&column.physical_name)?;
                if logical_names
                    .insert(column.logical_name.clone(), column.physical_name.clone())
                    .is_some()
                {
                    return Err(QueryError::Internal(format!(
                        "duplicate resident logical column: {}",
                        column.logical_name
                    )));
                }
                if column.hydrated && column.data_type != DataType::LargeBinary {
                    return Err(QueryError::Internal(format!(
                        "hydrated SQLite column must use LargeBinary: {}",
                        column.logical_name
                    )));
                }
                if !matches!(
                    column.data_type,
                    DataType::Utf8
                        | DataType::LargeUtf8
                        | DataType::Int64
                        | DataType::UInt64
                        | DataType::Float64
                        | DataType::Boolean
                        | DataType::Binary
                        | DataType::LargeBinary
                ) {
                    return Err(QueryError::Internal(format!(
                        "unsupported SQLite resident type for {}: {}",
                        column.logical_name, column.data_type
                    )));
                }
                let mut field = Field::new(
                    &column.logical_name,
                    column.data_type.clone(),
                    column.nullable,
                );
                if column.hydrated {
                    field = field.with_metadata(HashMap::from([
                        (
                            "ARROW:extension:name".to_owned(),
                            "boundary.baml_value".to_owned(),
                        ),
                        ("ARROW:extension:metadata".to_owned(), "v1".to_owned()),
                    ]));
                }
                Ok(field)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            name,
            physical_name,
            schema: Arc::new(datafusion::arrow::datatypes::Schema::new(fields)),
            columns,
        })
    }

    pub(crate) fn physical_column(&self, logical_name: &str) -> Option<&str> {
        self.columns
            .iter()
            .find(|column| column.logical_name == logical_name)
            .map(|column| column.physical_name.as_str())
    }

    pub(crate) fn column_mapping(&self) -> HashMap<String, String> {
        self.columns
            .iter()
            .filter(|column| !column.hydrated)
            .map(|column| {
                (
                    column.logical_name.clone(),
                    quote_identifier(&column.physical_name),
                )
            })
            .collect()
    }

    pub(crate) fn table_definition(&self) -> crate::TableDefinition {
        let mut definition = crate::TableDefinition::new(&self.name, self.schema.clone())
            .expect("validated SQLite table specification");
        if self.physical_column("project_id").is_some() {
            definition = definition.project_column("project_id");
        }
        for column in &self.columns {
            if column.hydrated {
                definition = definition.hydrated_column(&column.logical_name);
            }
        }
        definition
    }
}

pub fn standard_resident_table_specs() -> Result<Vec<SqliteResidentTableSpec>> {
    Ok(vec![
        SqliteResidentTableSpec::new(
            "processes",
            "processes",
            Arc::new(datafusion::arrow::datatypes::Schema::new(vec![
                datafusion::arrow::datatypes::Field::new("id", DataType::Utf8, false),
                datafusion::arrow::datatypes::Field::new("project_id", DataType::Utf8, false),
                datafusion::arrow::datatypes::Field::new("name", DataType::Utf8, true),
            ])),
        )?,
        SqliteResidentTableSpec::new(
            "threads",
            "threads",
            Arc::new(datafusion::arrow::datatypes::Schema::new(vec![
                datafusion::arrow::datatypes::Field::new("id", DataType::Utf8, false),
                datafusion::arrow::datatypes::Field::new("project_id", DataType::Utf8, false),
                datafusion::arrow::datatypes::Field::new("process_id", DataType::Utf8, true),
                datafusion::arrow::datatypes::Field::new("name", DataType::Utf8, true),
            ])),
        )?,
        SqliteResidentTableSpec::new(
            "ccts",
            "ccts",
            Arc::new(datafusion::arrow::datatypes::Schema::new(vec![
                datafusion::arrow::datatypes::Field::new("id", DataType::Utf8, false),
                datafusion::arrow::datatypes::Field::new("project_id", DataType::Utf8, false),
                datafusion::arrow::datatypes::Field::new("thread_id", DataType::Utf8, true),
                datafusion::arrow::datatypes::Field::new("name", DataType::Utf8, true),
            ])),
        )?,
    ])
}

/// A SQLite-backed table whose logical schema is supplied by the caller.
///
/// A column marked with [`SqliteColumnSpec::hydrated_value`] is read as a
/// `ValueId` from `SQLite` and exposed to `DataFusion` as hydrated JSON bytes.
#[derive(Clone)]
pub struct SqliteTableProvider {
    connection: Arc<Mutex<Connection>>,
    spec: SqliteResidentTableSpec,
    hydrator: Arc<dyn Hydrator>,
    query_context: QueryContext,
    batch_size: usize,
}

impl fmt::Debug for SqliteTableProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteTableProvider")
            .field("table", &self.spec.name)
            .field("batch_size", &self.batch_size)
            .finish_non_exhaustive()
    }
}

impl SqliteTableProvider {
    pub fn new(
        connection: Arc<Mutex<Connection>>,
        spec: SqliteResidentTableSpec,
        hydrator: Arc<dyn Hydrator>,
        query_context: QueryContext,
        batch_size: usize,
    ) -> Self {
        Self {
            connection,
            spec,
            hydrator,
            query_context,
            batch_size: batch_size.max(1),
        }
    }
}

#[async_trait::async_trait]
impl TableProvider for SqliteTableProvider {
    fn schema(&self) -> SchemaRef {
        self.spec.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        let columns = self.spec.column_mapping();
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
        let needs_values = self
            .spec
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                column.hydrated
                    && projection
                        .map(|projection| projection.contains(&index))
                        .unwrap_or(true)
            })
            .collect();
        let columns = self.spec.column_mapping();
        let generator = GenericBatchGenerator {
            connection: Arc::clone(&self.connection),
            spec: self.spec.clone(),
            hydrator: Arc::clone(&self.hydrator),
            query_context: self.query_context.clone(),
            predicates: filters
                .iter()
                .filter_map(|filter| crate::pushdown::to_sql_for_columns(filter, &columns))
                .collect(),
            needs_values,
            batch_size: self.batch_size,
            offset: 0,
        };
        let exec = LazyMemoryExec::try_new(self.schema(), vec![Arc::new(RwLock::new(generator))])?
            .with_projection(projection.cloned());
        Ok(Arc::new(exec))
    }
}

struct GenericBatchGenerator {
    connection: Arc<Mutex<Connection>>,
    spec: SqliteResidentTableSpec,
    hydrator: Arc<dyn Hydrator>,
    query_context: QueryContext,
    predicates: Vec<SqlPredicate>,
    needs_values: Vec<bool>,
    batch_size: usize,
    offset: usize,
}

impl fmt::Debug for GenericBatchGenerator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GenericBatchGenerator")
            .field("table", &self.spec.name)
            .field("batch_size", &self.batch_size)
            .field("offset", &self.offset)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for GenericBatchGenerator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let predicates = self
            .predicates
            .iter()
            .map(|predicate| predicate.sql.as_str())
            .collect::<Vec<_>>()
            .join(" AND ");
        write!(
            formatter,
            "SQLite {} batch_size={} offset={} predicates={predicates}",
            self.spec.name, self.batch_size, self.offset
        )
    }
}

impl LazyBatchGenerator for GenericBatchGenerator {
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
            spec: self.spec.clone(),
            hydrator: Arc::clone(&self.hydrator),
            query_context: self.query_context.clone(),
            predicates: self.predicates.clone(),
            needs_values: self.needs_values.clone(),
            batch_size: self.batch_size,
            offset: 0,
        }))
    }
}

impl GenericBatchGenerator {
    fn query_batch(&mut self) -> Result<Option<RecordBatch>> {
        self.query_context.check_cancelled()?;
        let sqlite_started = std::time::Instant::now();
        let fields = self.spec.schema.fields();
        let select_list = self
            .spec
            .columns
            .iter()
            .map(|column| quote_identifier(&column.physical_name))
            .collect::<Vec<_>>()
            .join(", ");
        let project_id = self
            .spec
            .physical_column("project_id")
            .ok_or_else(|| QueryError::Internal("table lacks project_id".to_owned()))?;
        let mut sql = format!(
            "SELECT {select_list} FROM {} WHERE {} = ?",
            quote_identifier(&self.spec.physical_name),
            quote_identifier(project_id),
        );
        let mut params = vec![Value::Text(self.query_context.project_id.to_string())];
        for predicate in &self.predicates {
            sql.push_str(" AND ");
            sql.push_str(&predicate.sql);
            params.extend(predicate.params.iter().cloned());
        }
        sql.push_str(" ORDER BY rowid LIMIT ? OFFSET ?");
        params.push(Value::Integer(
            i64::try_from(self.batch_size).map_err(|_| QueryError::ValueLimit)?,
        ));
        params.push(Value::Integer(
            i64::try_from(self.offset).map_err(|_| QueryError::ValueLimit)?,
        ));

        let (resident_columns, value_id_columns) = {
            let connection = self
                .connection
                .lock()
                .map_err(|_| QueryError::Internal("SQLite mutex poisoned".to_owned()))?;
            let mut statement = connection.prepare(&sql)?;
            let mut rows = statement.query(rusqlite::params_from_iter(params.iter()))?;
            let mut resident_columns = (0..fields.len())
                .map(|_| Vec::new())
                .collect::<Vec<Vec<Cell>>>();
            let mut value_id_columns = (0..fields.len())
                .map(|_| Vec::new())
                .collect::<Vec<Vec<Option<ValueId>>>>();
            while let Some(row) = rows.next()? {
                for (index, column) in self.spec.columns.iter().enumerate() {
                    if column.hydrated {
                        value_id_columns[index].push(read_value_id(row, index)?);
                    } else {
                        resident_columns[index].push(read_cell(
                            row,
                            index,
                            fields[index].data_type(),
                        )?);
                    }
                }
            }
            (resident_columns, value_id_columns)
        };
        self.query_context
            .metrics
            .record_sqlite_duration(sqlite_started.elapsed());
        let row_count = resident_columns
            .iter()
            .map(Vec::len)
            .chain(value_id_columns.iter().map(Vec::len))
            .max()
            .unwrap_or(0);
        if row_count == 0 {
            return Ok(None);
        }
        if self.offset.saturating_add(row_count) > self.query_context.budgets.max_candidate_rows {
            return Err(QueryError::ValueLimit);
        }
        self.offset += row_count;
        self.query_context
            .metrics
            .batches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.query_context
            .metrics
            .input_rows
            .fetch_add(row_count, std::sync::atomic::Ordering::Relaxed);

        let mut arrays = Vec::with_capacity(fields.len());
        for (index, column) in self.spec.columns.iter().enumerate() {
            if !column.hydrated {
                arrays.push(build_array(
                    fields[index].data_type(),
                    &resident_columns[index],
                )?);
                continue;
            }
            let values = self.hydrate_column(&value_id_columns[index], self.needs_values[index])?;
            let cells = values
                .into_iter()
                .map(|value| value.map_or(Cell::Null, Cell::Binary))
                .collect::<Vec<_>>();
            arrays.push(build_array(fields[index].data_type(), &cells)?);
        }
        Ok(Some(
            RecordBatch::try_new(self.spec.schema.clone(), arrays)
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

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct SqliteResidentTableProvider {
    connection: Arc<Mutex<Connection>>,
    spec: SqliteResidentTableSpec,
    query_context: QueryContext,
    batch_size: usize,
}

#[allow(dead_code)]
impl fmt::Debug for SqliteResidentTableProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteResidentTableProvider")
            .field("table", &self.spec.name)
            .field("batch_size", &self.batch_size)
            .finish_non_exhaustive()
    }
}

#[allow(dead_code)]
impl SqliteResidentTableProvider {
    pub(crate) fn new(
        connection: Arc<Mutex<Connection>>,
        spec: SqliteResidentTableSpec,
        query_context: QueryContext,
        batch_size: usize,
    ) -> Self {
        Self {
            connection,
            spec,
            query_context,
            batch_size: batch_size.max(1),
        }
    }
}

#[allow(dead_code)]
#[async_trait::async_trait]
impl TableProvider for SqliteResidentTableProvider {
    fn schema(&self) -> SchemaRef {
        self.spec.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        let columns = self.spec.column_mapping();
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
        let columns = self.spec.column_mapping();
        let generator = ResidentBatchGenerator {
            connection: Arc::clone(&self.connection),
            spec: self.spec.clone(),
            query_context: self.query_context.clone(),
            predicates: filters
                .iter()
                .filter_map(|filter| crate::pushdown::to_sql_for_columns(filter, &columns))
                .collect(),
            batch_size: self.batch_size,
            offset: 0,
        };
        let exec = LazyMemoryExec::try_new(self.schema(), vec![Arc::new(RwLock::new(generator))])?
            .with_projection(projection.cloned());
        Ok(Arc::new(exec))
    }
}

#[allow(dead_code)]
struct ResidentBatchGenerator {
    connection: Arc<Mutex<Connection>>,
    spec: SqliteResidentTableSpec,
    query_context: QueryContext,
    predicates: Vec<SqlPredicate>,
    batch_size: usize,
    offset: usize,
}

#[allow(dead_code)]
impl fmt::Debug for ResidentBatchGenerator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResidentBatchGenerator")
            .field("table", &self.spec.name)
            .field("batch_size", &self.batch_size)
            .field("offset", &self.offset)
            .finish_non_exhaustive()
    }
}

#[allow(dead_code)]
impl fmt::Display for ResidentBatchGenerator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let predicates = self
            .predicates
            .iter()
            .map(|predicate| predicate.sql.as_str())
            .collect::<Vec<_>>()
            .join(" AND ");
        write!(
            formatter,
            "SQLite {} batch_size={} offset={} predicates={predicates}",
            self.spec.name, self.batch_size, self.offset
        )
    }
}

#[allow(dead_code)]
impl LazyBatchGenerator for ResidentBatchGenerator {
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
            spec: self.spec.clone(),
            query_context: self.query_context.clone(),
            predicates: self.predicates.clone(),
            batch_size: self.batch_size,
            offset: 0,
        }))
    }
}

#[allow(dead_code)]
impl ResidentBatchGenerator {
    fn query_batch(&mut self) -> Result<Option<RecordBatch>> {
        self.query_context.check_cancelled()?;
        let fields = self.spec.schema.fields();
        let select_list = self
            .spec
            .columns
            .iter()
            .map(|column| quote_identifier(&column.physical_name))
            .collect::<Vec<_>>()
            .join(", ");
        let project_id = self
            .spec
            .physical_column("project_id")
            .ok_or_else(|| QueryError::Internal("resident table lacks project_id".to_owned()))?;
        let mut sql = format!(
            "SELECT {select_list} FROM {} WHERE {} = ?",
            quote_identifier(&self.spec.physical_name),
            quote_identifier(project_id),
        );
        let mut params = vec![Value::Text(self.query_context.project_id.to_string())];
        for predicate in &self.predicates {
            sql.push_str(" AND ");
            sql.push_str(&predicate.sql);
            params.extend(predicate.params.iter().cloned());
        }
        sql.push_str(" ORDER BY rowid LIMIT ? OFFSET ?");
        params.push(Value::Integer(
            i64::try_from(self.batch_size).map_err(|_| QueryError::ValueLimit)?,
        ));
        params.push(Value::Integer(
            i64::try_from(self.offset).map_err(|_| QueryError::ValueLimit)?,
        ));

        let columns = {
            let connection = self
                .connection
                .lock()
                .map_err(|_| QueryError::Internal("SQLite mutex poisoned".to_owned()))?;
            let mut statement = connection.prepare(&sql)?;
            let mut rows = statement.query(rusqlite::params_from_iter(params.iter()))?;
            let mut columns = (0..fields.len()).map(|_| Vec::new()).collect::<Vec<_>>();
            while let Some(row) = rows.next()? {
                for (index, field) in fields.iter().enumerate() {
                    columns[index].push(read_cell(row, index, field.data_type())?);
                }
            }
            columns
        };
        let row_count = columns.first().map_or(0, Vec::len);
        if row_count == 0 {
            return Ok(None);
        }
        if self.offset.saturating_add(row_count) > self.query_context.budgets.max_candidate_rows {
            return Err(QueryError::ValueLimit);
        }
        self.offset += row_count;
        self.query_context
            .metrics
            .batches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.query_context
            .metrics
            .input_rows
            .fetch_add(row_count, std::sync::atomic::Ordering::Relaxed);
        let arrays = fields
            .iter()
            .enumerate()
            .map(|(index, field)| build_array(field.data_type(), &columns[index]))
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(
            RecordBatch::try_new(self.spec.schema.clone(), arrays)
                .map_err(|error| QueryError::Internal(error.to_string()))?,
        ))
    }
}

#[derive(Debug)]
enum Cell {
    Null,
    Text(String),
    Int64(i64),
    UInt64(u64),
    Float64(f64),
    Boolean(bool),
    Binary(Vec<u8>),
}

fn read_cell(row: &Row<'_>, index: usize, data_type: &DataType) -> Result<Cell> {
    match data_type {
        DataType::Utf8 => Ok(row
            .get::<_, Option<String>>(index)?
            .map_or(Cell::Null, Cell::Text)),
        DataType::LargeUtf8 => Ok(row
            .get::<_, Option<String>>(index)?
            .map_or(Cell::Null, Cell::Text)),
        DataType::Int64 => Ok(row
            .get::<_, Option<i64>>(index)?
            .map_or(Cell::Null, Cell::Int64)),
        DataType::UInt64 => row
            .get::<_, Option<i64>>(index)?
            .map_or(Ok(Cell::Null), |value| {
                u64::try_from(value)
                    .map(Cell::UInt64)
                    .map_err(|_| QueryError::InvalidValueId("negative UInt64".to_owned()))
            }),
        DataType::Float64 => Ok(row
            .get::<_, Option<f64>>(index)?
            .map_or(Cell::Null, Cell::Float64)),
        DataType::Boolean => Ok(row
            .get::<_, Option<bool>>(index)?
            .map_or(Cell::Null, Cell::Boolean)),
        DataType::Binary | DataType::LargeBinary => Ok(row
            .get::<_, Option<Vec<u8>>>(index)?
            .map_or(Cell::Null, Cell::Binary)),
        other => Err(QueryError::Internal(format!(
            "unsupported SQLite resident type: {other}"
        ))),
    }
}

fn read_value_id(row: &Row<'_>, index: usize) -> Result<Option<ValueId>> {
    let bytes: Option<Vec<u8>> = row.get(index)?;
    match bytes {
        None => Ok(None),
        Some(bytes) if bytes.len() == ValueId::LEN => {
            Ok(Some(ValueId::from_bytes(bytes.try_into().map_err(
                |_| QueryError::InvalidValueId("wrong byte length".to_owned()),
            )?)))
        }
        Some(bytes) => Err(QueryError::InvalidValueId(format!(
            "expected {} bytes, got {}",
            ValueId::LEN,
            bytes.len()
        ))),
    }
}

fn build_array(data_type: &DataType, values: &[Cell]) -> Result<ArrayRef> {
    match data_type {
        DataType::Utf8 => {
            let mut builder = StringBuilder::new();
            for value in values {
                match value {
                    Cell::Text(value) => builder.append_value(value),
                    Cell::Null => builder.append_null(),
                    _ => return Err(type_mismatch(data_type)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::LargeUtf8 => {
            let mut builder = LargeStringBuilder::new();
            for value in values {
                match value {
                    Cell::Text(value) => builder.append_value(value),
                    Cell::Null => builder.append_null(),
                    _ => return Err(type_mismatch(data_type)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Int64 => {
            let mut builder = Int64Builder::new();
            for value in values {
                match value {
                    Cell::Int64(value) => builder.append_value(*value),
                    Cell::Null => builder.append_null(),
                    _ => return Err(type_mismatch(data_type)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::UInt64 => {
            let mut builder = UInt64Builder::new();
            for value in values {
                match value {
                    Cell::UInt64(value) => builder.append_value(*value),
                    Cell::Null => builder.append_null(),
                    _ => return Err(type_mismatch(data_type)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Float64 => {
            let mut builder = Float64Builder::new();
            for value in values {
                match value {
                    Cell::Float64(value) => builder.append_value(*value),
                    Cell::Null => builder.append_null(),
                    _ => return Err(type_mismatch(data_type)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Boolean => {
            let mut builder = BooleanBuilder::new();
            for value in values {
                match value {
                    Cell::Boolean(value) => builder.append_value(*value),
                    Cell::Null => builder.append_null(),
                    _ => return Err(type_mismatch(data_type)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Binary => {
            let mut builder = datafusion::arrow::array::BinaryBuilder::new();
            for value in values {
                match value {
                    Cell::Binary(value) => builder.append_value(value),
                    Cell::Null => builder.append_null(),
                    _ => return Err(type_mismatch(data_type)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::LargeBinary => {
            let mut builder = LargeBinaryBuilder::new();
            for value in values {
                match value {
                    Cell::Binary(value) => builder.append_value(value),
                    Cell::Null => builder.append_null(),
                    _ => return Err(type_mismatch(data_type)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        _ => Err(type_mismatch(data_type)),
    }
}

fn type_mismatch(data_type: &DataType) -> QueryError {
    QueryError::Internal(format!("SQLite value did not match Arrow type {data_type}"))
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
