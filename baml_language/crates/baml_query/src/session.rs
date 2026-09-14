//! One bound query session: catalog profile + scope + snapshot +
//! providers + budgets, assembled into a `DataFusion` context with the
//! BAML value planner, and executed with the mandatory terminal outcome
//! (TASK/baml-query-scope.md §5.1–§5.3).
//!
//! The public grammar is queries plus the read-only discovery statements
//! (`SHOW TABLES`, `SHOW COLUMNS`, `DESCRIBE`, `EXPLAIN <query>`). DDL,
//! DML, CREATE FUNCTION, multiple statements, and the reserved `__baml_`
//! prefix are rejected before planning (`invalid_sql`); backend-gated
//! functions are rejected after planning with `E_BACKEND_CAPABILITY`,
//! before any data read.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use datafusion::{
    arrow::record_batch::RecordBatch,
    catalog::{
        CatalogProvider as _, SchemaProvider, TableProvider, memory::MemoryCatalogProvider,
        view::ViewTable,
    },
    common::tree_node::{TreeNode, TreeNodeRecursion},
    execution::{context::SessionContext, session_state::SessionStateBuilder},
    logical_expr::{Expr, LogicalPlan},
    physical_plan::SendableRecordBatchStream,
    prelude::SessionConfig,
    sql::sqlparser::ast::Statement,
};
use futures::StreamExt as _;

use crate::{
    budget::{BudgetTracker, CancellationToken, QueryBudgets},
    capability::CapabilityRegistry,
    catalog::{CatalogProfile, RelationDef},
    error::{QueryError, QueryErrorCode, did_you_mean},
    outcome::QueryOutcome,
    provider::{RelationProviderFactory, TrustedRelation},
    scope::{QueryScope, Snapshot},
    value::{
        lowering::{BamlValuePlanner, ValueFunctions},
        resolver::{HydrationContext, ValueResolver},
    },
};

/// Bounded dedup cache entries per query (impl detail, not policy).
const HYDRATION_CACHE_ENTRIES: usize = 4096;

/// Everything needed to bind one query session.
pub struct QuerySessionBuilder {
    profile: CatalogProfile,
    scope: QueryScope,
    snapshot: Snapshot,
    budgets: QueryBudgets,
    cancel: CancellationToken,
    capabilities: CapabilityRegistry,
    resolver: Arc<dyn ValueResolver>,
    factory: Arc<dyn RelationProviderFactory>,
}

impl QuerySessionBuilder {
    #[must_use]
    pub fn new(
        profile: CatalogProfile,
        scope: QueryScope,
        snapshot: Snapshot,
        resolver: Arc<dyn ValueResolver>,
        factory: Arc<dyn RelationProviderFactory>,
    ) -> QuerySessionBuilder {
        QuerySessionBuilder {
            profile,
            scope,
            snapshot,
            budgets: QueryBudgets::unlimited(),
            cancel: CancellationToken::new(),
            capabilities: CapabilityRegistry::new(),
            resolver,
            factory,
        }
    }

    #[must_use]
    pub fn with_budgets(mut self, budgets: QueryBudgets) -> Self {
        self.budgets = budgets;
        self
    }

    #[must_use]
    pub fn with_cancellation(mut self, cancel: CancellationToken) -> Self {
        self.cancel = cancel;
        self
    }

    #[must_use]
    pub fn with_capabilities(mut self, capabilities: CapabilityRegistry) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Bind the session: snapshot-fixed lazy providers under catalog
    /// `baml` / schema `public`, value functions wired to this query's
    /// resolver and budget, planner/stubs registered, views planted.
    pub async fn build(self) -> Result<QuerySession, QueryError> {
        let tracker = BudgetTracker::new(self.budgets, self.cancel.clone());
        let hydration =
            HydrationContext::new(self.resolver, tracker.clone(), HYDRATION_CACHE_ENTRIES);
        let functions = ValueFunctions::new(hydration);

        let config = SessionConfig::new()
            .with_default_catalog_and_schema("baml", "public")
            .with_information_schema(true);
        // Our planner comes FIRST so value-typed subscripts/comparisons
        // are claimed before the core planners see them; the defaults must
        // still follow (`with_expr_planners` REPLACES them, and losing the
        // aggregate planner breaks e.g. `COUNT(*)`).
        let mut expr_planners: Vec<Arc<dyn datafusion::logical_expr::planner::ExprPlanner>> =
            vec![BamlValuePlanner::new(functions.clone())];
        expr_planners.extend(datafusion::execution::SessionStateDefaults::default_expr_planners());
        let state = SessionStateBuilder::new()
            .with_default_features()
            .with_config(config)
            .with_expr_planners(expr_planners)
            .build();
        let ctx = SessionContext::new_with_state(state);
        for udf in functions.all() {
            ctx.register_udf(udf.as_ref().clone());
        }
        for stub in self.capabilities.planning_stubs() {
            ctx.register_udf(stub.as_ref().clone());
        }

        let schema = Arc::new(BamlSchemaProvider::new(
            self.profile.relations(),
            self.snapshot.clone(),
            self.factory,
        ));
        let catalog = MemoryCatalogProvider::new();
        catalog
            .register_schema("public", schema.clone())
            .map_err(|e| internal(&e))?;
        ctx.register_catalog("baml", Arc::new(catalog));

        // Views plan over the relations just registered; every view of the
        // profile must plan (the golden gate re-checks this in tests).
        for view in self.profile.views() {
            let plan = ctx
                .state()
                .create_logical_plan(view.sql)
                .await
                .map_err(|e| {
                    internal(&e).with_remedy(format!("view `{}` failed to plan", view.name))
                })?;
            let table: Arc<dyn TableProvider> =
                Arc::new(ViewTable::new(plan, Some(view.sql.to_string())));
            schema.register_view(view.name, table.clone());
            schema.register_view(view.alias, table);
        }

        // §5.8: column docs are SELECT-able through `baml_columns`, so
        // agents can discover documentation without leaving SQL.
        schema.register_view(
            "baml_columns",
            columns_table(&self.profile).map_err(|e| internal(&e))?,
        );

        Ok(QuerySession {
            ctx,
            profile: self.profile,
            scope: self.scope,
            snapshot: self.snapshot,
            capabilities: self.capabilities,
            tracker,
            functions,
        })
    }
}

/// The `baml_columns` documentation relation: one row per visible column
/// of the profile.
fn columns_table(profile: &CatalogProfile) -> datafusion::common::Result<Arc<dyn TableProvider>> {
    use datafusion::arrow::{
        array::{BooleanBuilder, StringBuilder},
        datatypes::{DataType, Field, Schema},
        record_batch::RecordBatch,
    };

    let mut relation = StringBuilder::new();
    let mut column = StringBuilder::new();
    let mut data_type = StringBuilder::new();
    let mut nullable = BooleanBuilder::new();
    let mut key = BooleanBuilder::new();
    let mut virtual_ = BooleanBuilder::new();
    let mut role = StringBuilder::new();
    let mut doc = StringBuilder::new();
    for rel in profile.relations() {
        for col in &rel.columns {
            relation.append_value(rel.name);
            column.append_value(col.name);
            data_type.append_value(format!("{:?}", col.data_type));
            nullable.append_value(col.nullable);
            key.append_value(col.key);
            virtual_.append_value(col.value_role.is_some());
            role.append_option(col.value_role);
            doc.append_value(col.doc);
        }
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("relation", DataType::Utf8, false),
        Field::new("column", DataType::Utf8, false),
        Field::new("data_type", DataType::Utf8, false),
        Field::new("nullable", DataType::Boolean, false),
        Field::new("key", DataType::Boolean, false),
        Field::new("virtual", DataType::Boolean, false),
        Field::new("role", DataType::Utf8, true),
        Field::new("doc", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(relation.finish()),
            Arc::new(column.finish()),
            Arc::new(data_type.finish()),
            Arc::new(nullable.finish()),
            Arc::new(key.finish()),
            Arc::new(virtual_.finish()),
            Arc::new(role.finish()),
            Arc::new(doc.finish()),
        ],
    )?;
    Ok(Arc::new(datafusion::datasource::memory::MemTable::try_new(
        schema,
        vec![vec![batch]],
    )?))
}

/// The lazy schema provider (§5.2): lists the profile's visible
/// relations and views; instantiates a provider on first use and caches
/// it for the session. A relation the backend does not serve resolves to
/// an empty provider over the catalog schema (still queryable).
struct BamlSchemaProvider {
    relations: Vec<RelationDef>,
    snapshot: Snapshot,
    factory: Arc<dyn RelationProviderFactory>,
    tables: Mutex<HashMap<&'static str, Arc<dyn TableProvider>>>,
    views: Mutex<HashMap<String, Arc<dyn TableProvider>>>,
}

impl std::fmt::Debug for BamlSchemaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BamlSchemaProvider").finish_non_exhaustive()
    }
}

impl BamlSchemaProvider {
    fn new(
        relations: Vec<RelationDef>,
        snapshot: Snapshot,
        factory: Arc<dyn RelationProviderFactory>,
    ) -> BamlSchemaProvider {
        BamlSchemaProvider {
            relations,
            snapshot,
            factory,
            tables: Mutex::new(HashMap::new()),
            views: Mutex::new(HashMap::new()),
        }
    }

    fn register_view(&self, name: &str, table: Arc<dyn TableProvider>) {
        self.views
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(name.to_string(), table);
    }

    fn relation(&self, name: &str) -> Option<&RelationDef> {
        self.relations.iter().find(|r| r.names().any(|n| n == name))
    }
}

#[async_trait]
impl SchemaProvider for BamlSchemaProvider {
    fn table_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .relations
            .iter()
            .flat_map(RelationDef::names)
            .map(str::to_string)
            .collect();
        names.extend(
            self.views
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .keys()
                .cloned(),
        );
        names
    }

    async fn table(
        &self,
        name: &str,
    ) -> datafusion::common::Result<Option<Arc<dyn TableProvider>>> {
        if let Some(view) = self
            .views
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
        {
            return Ok(Some(view.clone()));
        }
        let Some(relation) = self.relation(name) else {
            return Ok(None);
        };
        let mut tables = self
            .tables
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(table) = tables.get(relation.name) {
            return Ok(Some(table.clone()));
        }
        let inner = self
            .factory
            .provider(relation, &self.snapshot)
            .map_err(|err| datafusion::common::DataFusionError::External(Box::new(err)))?;
        let table: Arc<dyn TableProvider> = Arc::new(TrustedRelation::new(relation, inner));
        tables.insert(relation.name, table.clone());
        Ok(Some(table))
    }

    fn table_exist(&self, name: &str) -> bool {
        self.relation(name).is_some()
            || self
                .views
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(name)
    }
}

/// A bound session. One session executes one statement at a time; every
/// execution ends in exactly one [`QueryOutcome`].
pub struct QuerySession {
    ctx: SessionContext,
    profile: CatalogProfile,
    scope: QueryScope,
    snapshot: Snapshot,
    capabilities: CapabilityRegistry,
    tracker: Arc<BudgetTracker>,
    functions: Arc<ValueFunctions>,
}

impl QuerySession {
    #[must_use]
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// The rendered catalog slice this session exposes (`--schema`).
    #[must_use]
    pub fn profile(&self) -> &CatalogProfile {
        &self.profile
    }

    /// Plan and start one portable SQL statement.
    ///
    /// A planning failure still produces a terminal outcome — take it
    /// from the returned error via the second tuple element.
    #[allow(clippy::result_large_err)] // Preserve the public result shape without adding boxing.
    pub async fn execute(&self, sql: &str) -> Result<QueryExecution, (QueryError, QueryOutcome)> {
        match self.plan_and_run(sql).await {
            Ok(stream) => Ok(QueryExecution {
                snapshot: self.snapshot.clone(),
                tracker: self.tracker.clone(),
                stream: Some(stream),
                terminal: None,
            }),
            Err(err) => {
                let outcome = QueryOutcome::ended(
                    self.snapshot.clone(),
                    self.tracker.value_evaluations(),
                    self.tracker.rows_streamed(),
                    &err,
                );
                Err((err, outcome))
            }
        }
    }

    async fn plan_and_run(&self, sql: &str) -> Result<SendableRecordBatchStream, QueryError> {
        let statement = gatekeep(sql)?;
        let plan = self
            .ctx
            .state()
            .statement_to_plan(statement)
            .await
            .map_err(|e| self.plan_error(e))?;
        // Bare value columns in projections render; handles never leak.
        let plan = crate::value::lowering::rewrite_bare_value_columns(plan, &self.functions)
            .map_err(|e| self.plan_error(e))?;
        self.check_capabilities(&plan)?;
        self.check_value_authorization(&plan)?;
        self.tracker.checkpoint()?;
        let dataframe = self
            .ctx
            .execute_logical_plan(plan)
            .await
            .map_err(|e| self.plan_error(e))?;
        dataframe.execute_stream().await.map_err(|e| exec_error(&e))
    }

    /// Map a `DataFusion` planning error to a typed error, restoring a
    /// provider's own typed error and attaching a did-you-mean remedy on
    /// unknown tables (house style from `baml describe`).
    fn plan_error(&self, err: datafusion::common::DataFusionError) -> QueryError {
        use datafusion::common::DataFusionError;
        match err {
            DataFusionError::External(boxed) => match boxed.downcast::<QueryError>() {
                Ok(typed) => *typed,
                Err(other) => QueryError::new(QueryErrorCode::Internal, other.to_string()),
            },
            DataFusionError::Plan(msg) => {
                let mut error = QueryError::invalid_sql(msg.clone());
                if let Some(unknown) = unknown_table_name(&msg) {
                    let names: Vec<&str> = self
                        .profile
                        .relations()
                        .iter()
                        .flat_map(RelationDef::names)
                        .chain(self.profile.views().iter().map(|v| v.alias))
                        .collect();
                    if let Some(remedy) = did_you_mean(&unknown, names.into_iter()) {
                        error = error.with_remedy(remedy);
                    }
                }
                error
            }
            err @ DataFusionError::SchemaError(..) => QueryError::invalid_sql(err.to_string()),
            DataFusionError::SQL(e, _) => QueryError::invalid_sql(format!("SQL error: {e}")),
            DataFusionError::Context(_, inner) | DataFusionError::Diagnostic(_, inner) => {
                self.plan_error(*inner)
            }
            other => QueryError::new(QueryErrorCode::Internal, other.to_string()),
        }
    }
}

/// Grammar gate: one statement, queries and read-only discovery only,
/// reserved names rejected.
fn gatekeep(sql: &str) -> Result<datafusion::sql::parser::Statement, QueryError> {
    // Name the rejection even when the dialect cannot parse the
    // statement far enough to classify it.
    if sql
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("create function")
    {
        return Err(QueryError::invalid_sql(
            "CREATE FUNCTION is not part of the v1 surface: only \
                 platform-owned functions exist",
        ));
    }
    let dialect = datafusion::sql::sqlparser::dialect::GenericDialect {};
    let mut statements = datafusion::sql::parser::DFParser::parse_sql_with_dialect(sql, &dialect)
        .map_err(|e| QueryError::invalid_sql(format!("SQL parse error: {e}")))?;
    if statements.len() != 1 {
        return Err(QueryError::invalid_sql(format!(
            "expected exactly one statement, got {}",
            statements.len()
        )));
    }
    let statement = statements.pop_front().expect("one statement");
    match &statement {
        datafusion::sql::parser::Statement::Statement(inner) => match inner.as_ref() {
            Statement::Query(_) => {}
            // Read-only discovery statements (§5.8): SHOW TABLES,
            // SHOW COLUMNS, DESCRIBE, EXPLAIN <query>.
            Statement::ShowTables { .. } | Statement::ShowColumns { .. } => {}
            Statement::ExplainTable { .. } => {}
            Statement::Explain {
                statement: explained,
                ..
            } => {
                if !matches!(explained.as_ref(), Statement::Query(_)) {
                    return Err(QueryError::invalid_sql(
                        "EXPLAIN is supported for queries only",
                    ));
                }
            }
            Statement::CreateFunction { .. } => {
                return Err(QueryError::invalid_sql(
                    "CREATE FUNCTION is not part of the v1 surface: only \
                         platform-owned functions exist",
                ));
            }
            other => {
                return Err(QueryError::invalid_sql(format!(
                    "only queries are part of the public surface; got: {}",
                    statement_kind(other)
                )));
            }
        },
        _ => {
            return Err(QueryError::invalid_sql(
                "only portable SQL queries are part of the public surface",
            ));
        }
    }
    // The __baml_ prefix is reserved for planner-planted internals.
    // Scan the rendered statement OUTSIDE single-quoted string literals,
    // so data queries mentioning the prefix in a string
    // (`WHERE fqn != '__baml_x'`) stay legal.
    let rendered = statement.to_string().to_ascii_lowercase();
    if contains_outside_string_literals(&rendered, crate::value::lowering::INTERNAL_FN_PREFIX) {
        return Err(QueryError::invalid_sql(
            "the __baml_ function prefix is reserved for internal lowering",
        ));
    }
    Ok(statement)
}

/// Does `needle` occur in `sql` outside single-quoted string literals?
/// (Standard SQL escaping doubles the quote, which this walk treats as
/// leaving and immediately re-entering a literal — either way the
/// content stays "inside".)
fn contains_outside_string_literals(sql: &str, needle: &str) -> bool {
    let mut in_string = false;
    let mut scan = String::with_capacity(sql.len());
    for ch in sql.chars() {
        if ch == '\'' {
            in_string = !in_string;
            scan.push(' ');
        } else {
            scan.push(if in_string { ' ' } else { ch });
        }
    }
    scan.contains(needle)
}

/// Walk every scalar-function reference in `plan`, INCLUDING the plans
/// embedded in subquery expressions (`EXISTS` / `IN` / scalar
/// subqueries): the plain `apply` family deliberately skips those, which
/// is exactly where a hostile query would tuck a gated function. The
/// first `Some(err)` from `check` aborts the walk.
fn scan_scalar_functions(
    plan: &LogicalPlan,
    mut check: impl FnMut(&str) -> Option<QueryError>,
) -> Result<(), QueryError> {
    let mut violation: Option<QueryError> = None;
    let _ = plan.apply_with_subqueries(|node| {
        let _ = node.apply_expressions(|expr| {
            let _ = expr.apply(|e| {
                if let Expr::ScalarFunction(call) = e
                    && let Some(err) = check(call.func.name())
                {
                    violation = Some(err);
                    return Ok(TreeNodeRecursion::Stop);
                }
                Ok(TreeNodeRecursion::Continue)
            });
            Ok(if violation.is_some() {
                TreeNodeRecursion::Stop
            } else {
                TreeNodeRecursion::Continue
            })
        });
        Ok(if violation.is_some() {
            TreeNodeRecursion::Stop
        } else {
            TreeNodeRecursion::Continue
        })
    });
    violation.map_or(Ok(()), Err)
}

impl QuerySession {
    /// Reject backend-gated functions before any read.
    fn check_capabilities(&self, plan: &LogicalPlan) -> Result<(), QueryError> {
        let backend = self.scope.backend;
        let capabilities = &self.capabilities;
        scan_scalar_functions(plan, |name| capabilities.check(name, backend).err())
    }

    /// Value reads fail before execution when the scope lacks the right.
    fn check_value_authorization(&self, plan: &LogicalPlan) -> Result<(), QueryError> {
        if self.scope.value_read_allowed {
            return Ok(());
        }
        scan_scalar_functions(plan, |name| {
            name.starts_with(crate::value::lowering::INTERNAL_FN_PREFIX)
                .then(|| {
                    QueryError::new(
                        QueryErrorCode::AuthorizationDenied,
                        "this scope may not read value content",
                    )
                })
        })
    }
}

/// Extract `xyz` from a "table 'baml.public.xyz' not found" message.
fn unknown_table_name(message: &str) -> Option<String> {
    let (_, tail) = message.split_once("table '")?;
    let (path, _) = tail.split_once('\'')?;
    Some(path.rsplit('.').next().unwrap_or(path).to_string())
}

fn statement_kind(statement: &Statement) -> String {
    // First tokens of the canonical rendering name the verb without
    // leaking statement contents into error text.
    statement
        .to_string()
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

fn internal(err: &datafusion::common::DataFusionError) -> QueryError {
    QueryError::new(QueryErrorCode::Internal, err.to_string())
}

fn exec_error(err: &datafusion::common::DataFusionError) -> QueryError {
    QueryError::new(QueryErrorCode::Internal, err.to_string())
}

/// One running query: a budget-checked batch stream plus the mandatory
/// terminal outcome. Exactly one outcome exists per execution — it is
/// produced by [`QueryExecution::finish`] after the stream ends (or by
/// the failed-planning path in [`QuerySession::execute`]).
pub struct QueryExecution {
    snapshot: Snapshot,
    tracker: Arc<BudgetTracker>,
    stream: Option<SendableRecordBatchStream>,
    terminal: Option<QueryError>,
}

impl QueryExecution {
    /// Next result batch. `None` = end of stream (natural or terminal —
    /// call [`QueryExecution::finish`] for the authoritative outcome).
    pub async fn next_batch(&mut self) -> Option<RecordBatch> {
        if self.terminal.is_some() {
            return None;
        }
        let stream = self.stream.as_mut()?;
        loop {
            if let Err(err) = self.tracker.checkpoint() {
                self.terminal = Some(err);
                self.stream = None;
                return None;
            }
            match stream.next().await {
                Some(Ok(batch)) => {
                    if let Err(err) = self.tracker.count_result_rows(batch.num_rows() as u64) {
                        self.terminal = Some(err);
                        self.stream = None;
                        return None;
                    }
                    if batch.num_rows() == 0 {
                        continue;
                    }
                    return Some(batch);
                }
                Some(Err(err)) => {
                    self.terminal = Some(exec_error(&err));
                    self.stream = None;
                    return None;
                }
                None => {
                    self.stream = None;
                    return None;
                }
            }
        }
    }

    /// Consume the execution into its terminal outcome. The stream need
    /// not be fully drained — an early finish is a completed-early
    /// caller, and the outcome reports exactly what was streamed.
    #[must_use]
    pub fn finish(self) -> QueryOutcome {
        let values = self.tracker.value_evaluations();
        let rows = self.tracker.rows_streamed();
        match &self.terminal {
            Some(err) => QueryOutcome::ended(self.snapshot.clone(), values, rows, err),
            None => QueryOutcome::completed(self.snapshot.clone(), values, rows),
        }
    }

    /// The typed terminal error, when the stream ended on one.
    #[must_use]
    pub fn terminal_error(&self) -> Option<&QueryError> {
        self.terminal.as_ref()
    }
}
