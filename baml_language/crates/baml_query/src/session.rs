//! One bound query session: catalog + scope + snapshot + providers +
//! budgets, assembled into a DataFusion context with the BAML value
//! planner, and executed with the mandatory terminal outcome (D13).
//!
//! The public grammar is queries only. DDL, DML, CREATE FUNCTION,
//! multiple statements, and the reserved `__baml_` prefix are rejected
//! before planning (invalid_sql); backend-gated functions are rejected
//! after planning with E_BACKEND_CAPABILITY, before any data read (D4).

use std::sync::Arc;

use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
use datafusion::execution::context::SessionContext;
use datafusion::execution::session_state::SessionStateBuilder;
use datafusion::logical_expr::{Expr, LogicalPlan};
use datafusion::physical_plan::SendableRecordBatchStream;
use datafusion::sql::sqlparser::ast::Statement;
use futures::StreamExt as _;

use crate::budget::{BudgetTracker, CancellationToken, QueryBudgets};
use crate::capability::CapabilityRegistry;
use crate::catalog::LogicalCatalog;
use crate::error::{QueryError, QueryErrorCode};
use crate::outcome::QueryOutcome;
use crate::provider::{RelationProviderFactory, TrustedRelation};
use crate::scope::{QueryScope, Snapshot};
use crate::value::lowering::{BamlValuePlanner, ValueFunctions};
use crate::value::resolver::{HydrationContext, ValueResolver};

/// Bounded dedup cache entries per query (impl detail, not policy).
const HYDRATION_CACHE_ENTRIES: usize = 4096;

/// Everything needed to bind one query session.
pub struct QuerySessionBuilder {
    catalog: LogicalCatalog,
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
        catalog: LogicalCatalog,
        scope: QueryScope,
        snapshot: Snapshot,
        resolver: Arc<dyn ValueResolver>,
        factory: Arc<dyn RelationProviderFactory>,
    ) -> QuerySessionBuilder {
        QuerySessionBuilder {
            catalog,
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

    /// Bind the session: snapshot-fixed providers, value functions wired
    /// to this query's resolver and budget, planner and stubs registered.
    pub fn build(self) -> Result<QuerySession, QueryError> {
        let tracker = BudgetTracker::new(self.budgets, self.cancel.clone());
        let hydration =
            HydrationContext::new(self.resolver, tracker.clone(), HYDRATION_CACHE_ENTRIES);
        let functions = ValueFunctions::new(hydration);

        let state = SessionStateBuilder::new()
            .with_default_features()
            .with_expr_planners(vec![BamlValuePlanner::new(functions.clone())])
            .build();
        let ctx = SessionContext::new_with_state(state);
        for udf in functions.all() {
            ctx.register_udf(udf.as_ref().clone());
        }
        for stub in self.capabilities.planning_stubs() {
            ctx.register_udf(stub.as_ref().clone());
        }

        for relation in &self.catalog.relations {
            let inner = self.factory.provider(relation, &self.snapshot)?;
            let trusted = Arc::new(TrustedRelation::new(relation, inner));
            ctx.register_table(relation.name, trusted.clone())
                .map_err(internal)?;
            ctx.register_table(relation.alias, trusted)
                .map_err(internal)?;
        }

        Ok(QuerySession {
            ctx,
            scope: self.scope,
            snapshot: self.snapshot,
            capabilities: self.capabilities,
            tracker,
            functions,
        })
    }
}

/// A bound session. One session executes one statement at a time; every
/// execution ends in exactly one [`QueryOutcome`].
pub struct QuerySession {
    ctx: SessionContext,
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

    /// Plan and start one portable SQL statement.
    ///
    /// A planning failure still produces a terminal outcome — take it
    /// from the returned error via [`QueryExecution::failed`].
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
        let statement = self.gatekeep(sql)?;
        let plan = self
            .ctx
            .state()
            .statement_to_plan(statement)
            .await
            .map_err(plan_error)?;
        // Bare value columns in projections render; handles never leak.
        let plan = crate::value::lowering::rewrite_bare_value_columns(plan, &self.functions)
            .map_err(plan_error)?;
        self.check_capabilities(&plan)?;
        self.check_value_authorization(&plan)?;
        self.tracker.checkpoint()?;
        let dataframe = self
            .ctx
            .execute_logical_plan(plan)
            .await
            .map_err(plan_error)?;
        dataframe.execute_stream().await.map_err(exec_error)
    }

    /// Grammar gate: one statement, queries only, reserved names rejected.
    fn gatekeep(&self, sql: &str) -> Result<datafusion::sql::parser::Statement, QueryError> {
        // D14: name the rejection even when the dialect cannot parse the
        // statement far enough to classify it.
        if sql
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("create function")
        {
            return Err(QueryError::invalid_sql(
                "CREATE FUNCTION is not part of the v1 surface: only \
                 platform-owned functions exist (D14)",
            ));
        }
        let state = self.ctx.state();
        let dialect = datafusion::sql::sqlparser::dialect::GenericDialect {};
        let mut statements =
            datafusion::sql::parser::DFParser::parse_sql_with_dialect(sql, &dialect)
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
                Statement::CreateFunction { .. } => {
                    return Err(QueryError::invalid_sql(
                        "CREATE FUNCTION is not part of the v1 surface: only \
                         platform-owned functions exist (D14)",
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
        let rendered = statement.to_string().to_ascii_lowercase();
        if rendered.contains("__baml_") {
            return Err(QueryError::invalid_sql(
                "the __baml_ function prefix is reserved for internal lowering",
            ));
        }
        let _ = state;
        Ok(statement)
    }

    /// D4: reject backend-gated functions before any read.
    fn check_capabilities(&self, plan: &LogicalPlan) -> Result<(), QueryError> {
        let mut violation: Option<QueryError> = None;
        let backend = self.scope.backend;
        let capabilities = &self.capabilities;
        let _ = plan.apply(|node| {
            let mut recursion = TreeNodeRecursion::Continue;
            let _ = node.apply_expressions(|expr| {
                let mut expr_recursion = TreeNodeRecursion::Continue;
                let _ = expr.apply(|e| {
                    if let Expr::ScalarFunction(call) = e
                        && let Err(err) = capabilities.check(call.func.name(), backend)
                    {
                        violation = Some(err);
                        expr_recursion = TreeNodeRecursion::Stop;
                        return Ok(TreeNodeRecursion::Stop);
                    }
                    Ok(TreeNodeRecursion::Continue)
                });
                if violation.is_some() {
                    recursion = TreeNodeRecursion::Stop;
                }
                Ok(expr_recursion)
            });
            Ok(recursion)
        });
        match violation {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Value reads fail before execution when the scope lacks the right.
    fn check_value_authorization(&self, plan: &LogicalPlan) -> Result<(), QueryError> {
        if self.scope.value_read_allowed {
            return Ok(());
        }
        let mut needs_values = false;
        let _ = plan.apply(|node| {
            let mut recursion = TreeNodeRecursion::Continue;
            let _ = node.apply_expressions(|expr| {
                let _ = expr.apply(|e| {
                    if let Expr::ScalarFunction(call) = e
                        && call.func.name().starts_with("__baml_")
                    {
                        needs_values = true;
                        return Ok(TreeNodeRecursion::Stop);
                    }
                    Ok(TreeNodeRecursion::Continue)
                });
                if needs_values {
                    recursion = TreeNodeRecursion::Stop;
                }
                Ok(TreeNodeRecursion::Continue)
            });
            Ok(recursion)
        });
        if needs_values {
            Err(QueryError::new(
                QueryErrorCode::AuthorizationDenied,
                "this scope may not read value content",
            ))
        } else {
            Ok(())
        }
    }
}

fn statement_kind(statement: &Statement) -> String {
    // First token of the canonical rendering names the verb without
    // leaking statement contents into error text.
    statement
        .to_string()
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

fn internal(err: datafusion::common::DataFusionError) -> QueryError {
    QueryError::new(QueryErrorCode::Internal, err.to_string())
}

fn plan_error(err: datafusion::common::DataFusionError) -> QueryError {
    match err {
        datafusion::common::DataFusionError::Plan(msg) => QueryError::invalid_sql(msg),
        err @ datafusion::common::DataFusionError::SchemaError(..) => {
            QueryError::invalid_sql(err.to_string())
        }
        datafusion::common::DataFusionError::SQL(e, _) => {
            QueryError::invalid_sql(format!("SQL error: {e}"))
        }
        other => QueryError::new(QueryErrorCode::Internal, other.to_string()),
    }
}

fn exec_error(err: datafusion::common::DataFusionError) -> QueryError {
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
                    self.terminal = Some(exec_error(err));
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

    /// Consume the execution into its terminal outcome (D13). The stream
    /// need not be fully drained — an early finish is a completed-early
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
