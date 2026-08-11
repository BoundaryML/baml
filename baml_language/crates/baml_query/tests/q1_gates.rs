//! Q1 freeze-gate tests (09-delivery-plan §Q1).
//!
//! These pin the public contract against an in-memory fixture backend:
//! natural value expressions and their internal rewrites; whole-value
//! equality that is never handle-byte equality; typed unavailability and
//! outcome reconciliation; forbidden constructs; planning-time backend
//! capability failures; query-global budgets and cancellation; and the
//! final-limit rule (a limit never reaches the provider when a value
//! predicate is in play).

use std::sync::{Arc, Mutex};

use baml_query::budget::{CancellationToken, QueryBudgets};
use baml_query::capability::CapabilityRegistry;
use baml_query::catalog::{self, RelationDef};
use baml_query::error::QueryErrorCode;
use baml_query::outcome::{ResultState, UnavailableReason};
use baml_query::provider::RelationProviderFactory;
use baml_query::scope::{Backend, QueryScope, Snapshot};
use baml_query::session::QuerySessionBuilder;
use baml_query::value::resolver::{DecodeCaps, Resolved, ValueResolver};
use bex_events::store::canon::{self, CanonValue};
use datafusion::arrow::array::{
    Array as _, ArrayRef, BinaryBuilder, ListBuilder, StringBuilder, TimestampNanosecondBuilder,
    UInt32Builder, UInt64Builder,
};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::common::Result as DfResult;
use datafusion::datasource::memory::MemTable;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::ExecutionPlan;

// ── fixture backend ────────────────────────────────────────────────────

/// One retained-call fixture row (only the fields the tests vary).
#[derive(Clone, Default)]
struct CallRow {
    run_id: &'static str,
    call_id: u64,
    definition_key: Option<&'static str>,
    status: &'static str,
    args_state: &'static str,
    /// Handle bytes for the `args` role (None = role not applicable).
    args_handle: Option<&'static [u8]>,
}

fn retained_calls_batch(relation: &RelationDef, rows: &[CallRow]) -> RecordBatch {
    let schema = relation.schema();
    let mut arrays: Vec<ArrayRef> = Vec::new();
    for column in &relation.columns {
        let array: ArrayRef = match column.name {
            "run_id" => {
                let mut b = StringBuilder::new();
                rows.iter().for_each(|r| b.append_value(r.run_id));
                Arc::new(b.finish())
            }
            "call_id" => {
                let mut b = UInt64Builder::new();
                rows.iter().for_each(|r| b.append_value(r.call_id));
                Arc::new(b.finish())
            }
            "thread_id" => {
                let mut b = UInt64Builder::new();
                rows.iter().for_each(|_| b.append_value(1));
                Arc::new(b.finish())
            }
            "parent_call_id" | "duration_ns" => {
                let mut b = UInt64Builder::new();
                rows.iter().for_each(|_| b.append_null());
                Arc::new(b.finish())
            }
            "node_id" | "capture_policy_version" => {
                let mut b = UInt32Builder::new();
                rows.iter().for_each(|_| b.append_null());
                Arc::new(b.finish())
            }
            "definition_key" => {
                let mut b = StringBuilder::new();
                rows.iter().for_each(|r| b.append_option(r.definition_key));
                Arc::new(b.finish())
            }
            "started_at" | "ended_at" => {
                let mut b = TimestampNanosecondBuilder::new().with_timezone("UTC");
                rows.iter().for_each(|_| b.append_null());
                Arc::new(b.finish())
            }
            "status" => {
                let mut b = StringBuilder::new();
                rows.iter().for_each(|r| b.append_value(r.status));
                Arc::new(b.finish())
            }
            "retention_reasons" | "exact_window_ids" | "evidence_ids" => {
                let mut b = ListBuilder::new(StringBuilder::new()).with_field(
                    datafusion::arrow::datatypes::Field::new(
                        "item",
                        datafusion::arrow::datatypes::DataType::Utf8,
                        false,
                    ),
                );
                rows.iter().for_each(|_| {
                    b.append(true);
                });
                Arc::new(b.finish())
            }
            "args_state" => {
                let mut b = StringBuilder::new();
                rows.iter().for_each(|r| b.append_value(r.args_state));
                Arc::new(b.finish())
            }
            "return_state" | "error_state" => {
                let mut b = StringBuilder::new();
                rows.iter().for_each(|_| b.append_value("not_applicable"));
                Arc::new(b.finish())
            }
            "args" => {
                let mut b = BinaryBuilder::new();
                rows.iter().for_each(|r| b.append_option(r.args_handle));
                Arc::new(b.finish())
            }
            "return" | "error" => {
                let mut b = BinaryBuilder::new();
                rows.iter().for_each(|_| b.append_null());
                Arc::new(b.finish())
            }
            other => panic!("fixture does not know column {other}"),
        };
        arrays.push(array);
    }
    RecordBatch::try_new(schema, arrays).expect("fixture batch matches catalog schema")
}

/// Fixture resolver: handle bytes → canonical values or typed gaps.
#[derive(Default)]
struct FixtureResolver {
    values: Vec<(&'static [u8], CanonValue)>,
    unavailable: Vec<(&'static [u8], UnavailableReason)>,
    /// Handles whose canonical CID the backend can prove without decode.
    cids: Vec<(&'static [u8], [u8; 32])>,
    cid_bodies: Vec<([u8; 32], CanonValue)>,
}

impl ValueResolver for FixtureResolver {
    fn resolve(&self, handle: &[u8], _caps: DecodeCaps) -> Resolved {
        if let Some((_, reason)) = self.unavailable.iter().find(|(h, _)| *h == handle) {
            return Resolved::Unavailable(*reason);
        }
        match self.values.iter().find(|(h, _)| *h == handle) {
            Some((_, value)) => Resolved::Value(Arc::new(value.clone())),
            None => Resolved::Unavailable(UnavailableReason::Lost),
        }
    }

    fn resolve_cid(&self, cid: &[u8; 32], _caps: DecodeCaps) -> Resolved {
        match self.cid_bodies.iter().find(|(c, _)| c == cid) {
            Some((_, value)) => Resolved::Value(Arc::new(value.clone())),
            None => Resolved::Unavailable(UnavailableReason::Lost),
        }
    }

    fn canonical_cid(&self, handle: &[u8]) -> Option<[u8; 32]> {
        self.cids
            .iter()
            .find(|(h, _)| *h == handle)
            .map(|(_, cid)| *cid)
    }
}

/// Records the `limit` each scan received — the final-limit gate probe.
#[derive(Debug)]
struct RecordingProvider {
    inner: Arc<dyn TableProvider>,
    scans: Arc<Mutex<Vec<Option<usize>>>>,
}

#[async_trait::async_trait]
impl TableProvider for RecordingProvider {
    fn schema(&self) -> datafusion::arrow::datatypes::SchemaRef {
        self.inner.schema()
    }
    fn table_type(&self) -> TableType {
        self.inner.table_type()
    }
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DfResult<Vec<TableProviderFilterPushDown>> {
        self.inner.supports_filters_pushdown(filters)
    }
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        self.scans.lock().unwrap().push(limit);
        self.inner.scan(state, projection, filters, limit).await
    }
}

struct FixtureFactory {
    calls: Vec<CallRow>,
    scans: Arc<Mutex<Vec<Option<usize>>>>,
}

impl RelationProviderFactory for FixtureFactory {
    fn provider(
        &self,
        relation: &RelationDef,
        _snapshot: &Snapshot,
    ) -> Result<Option<Arc<dyn TableProvider>>, baml_query::QueryError> {
        if relation.name != "retained_calls_v1" {
            return Ok(None);
        }
        let batch = retained_calls_batch(relation, &self.calls);
        let mem =
            MemTable::try_new(relation.schema(), vec![vec![batch]]).expect("fixture MemTable");
        Ok(Some(Arc::new(RecordingProvider {
            inner: Arc::new(mem),
            scans: self.scans.clone(),
        })))
    }
}

struct Fixture {
    session: baml_query::QuerySession,
    scans: Arc<Mutex<Vec<Option<usize>>>>,
}

fn snapshot() -> Snapshot {
    Snapshot {
        catalog_version: "v1".to_string(),
        generation: "fixture-1".to_string(),
        projected_through: None,
    }
}

fn fixture(calls: Vec<CallRow>, resolver: FixtureResolver) -> Fixture {
    fixture_with(
        calls,
        resolver,
        QueryBudgets::unlimited(),
        CancellationToken::new(),
        CapabilityRegistry::new(),
        QueryScope::local(),
    )
}

fn fixture_with(
    calls: Vec<CallRow>,
    resolver: FixtureResolver,
    budgets: QueryBudgets,
    cancel: CancellationToken,
    capabilities: CapabilityRegistry,
    scope: QueryScope,
) -> Fixture {
    let scans = Arc::new(Mutex::new(Vec::new()));
    let factory = Arc::new(FixtureFactory {
        calls,
        scans: scans.clone(),
    });
    let session = QuerySessionBuilder::new(
        catalog::catalog_v1(),
        scope,
        snapshot(),
        Arc::new(resolver),
        factory,
    )
    .with_budgets(budgets)
    .with_cancellation(cancel)
    .with_capabilities(capabilities)
    .build()
    .expect("fixture session builds");
    Fixture { session, scans }
}

fn customer(age: i64) -> CanonValue {
    CanonValue::Map(vec![(
        "customer".to_string(),
        CanonValue::Map(vec![
            ("age".to_string(), CanonValue::Int(age)),
            ("name".to_string(), CanonValue::String("ada".to_string())),
        ]),
    )])
}

/// Collect one column of u64 call ids across all batches.
async fn call_ids(execution: &mut baml_query::QueryExecution) -> Vec<u64> {
    use datafusion::arrow::array::UInt64Array;
    let mut out = Vec::new();
    while let Some(batch) = execution.next_batch().await {
        let col = batch
            .column_by_name("call_id")
            .expect("call_id column")
            .as_any()
            .downcast_ref::<UInt64Array>()
            .expect("u64")
            .clone();
        out.extend(col.iter().flatten());
    }
    out
}

// ── natural value expressions ──────────────────────────────────────────

#[tokio::test]
async fn nested_subscript_comparison_filters_by_canonical_semantics() {
    let fx = fixture(
        vec![
            CallRow {
                run_id: "r1",
                call_id: 1,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-young"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 2,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-old"),
                ..CallRow::default()
            },
        ],
        FixtureResolver {
            values: vec![(b"h-young", customer(22)), (b"h-old", customer(41))],
            ..FixtureResolver::default()
        },
    );
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM retained_calls_v1 \
             WHERE args['customer']['age'] >= 30",
        )
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, vec![2]);
    let outcome = run.finish();
    assert_eq!(outcome.result_state, ResultState::Complete);
    assert_eq!(outcome.value_evaluations.attempted, 2);
    assert_eq!(outcome.value_evaluations.available, 2);
}

#[tokio::test]
async fn args_root_is_a_named_argument_object() {
    let fx = fixture(vec![], FixtureResolver::default());
    let err = fx
        .session
        .execute("SELECT call_id FROM retained_calls_v1 WHERE args[0] = 1")
        .await
        .err()
        .expect("numeric subscript on the args root is a planning error")
        .0;
    assert_eq!(err.code, QueryErrorCode::InvalidSql);
    assert!(
        err.message.contains("named-argument object"),
        "remedy names the freeze: {}",
        err.message
    );
}

#[tokio::test]
async fn absent_path_and_captured_null_are_complete_nonmatches() {
    // One row whose args lack the requested key, one whose leaf is a
    // captured null: both are non-matches, the result stays COMPLETE.
    let fx = fixture(
        vec![
            CallRow {
                run_id: "r1",
                call_id: 1,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-nokey"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 2,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-null"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 3,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-match"),
                ..CallRow::default()
            },
        ],
        FixtureResolver {
            values: vec![
                (b"h-nokey", CanonValue::Map(vec![])),
                (
                    b"h-null",
                    CanonValue::Map(vec![(
                        "customer".to_string(),
                        CanonValue::Map(vec![("age".to_string(), CanonValue::Null)]),
                    )]),
                ),
                (b"h-match", customer(50)),
            ],
            ..FixtureResolver::default()
        },
    );
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM retained_calls_v1 \
             WHERE args['customer']['age'] >= 30",
        )
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, vec![3]);
    let outcome = run.finish();
    assert_eq!(
        outcome.result_state,
        ResultState::Complete,
        "absent path / captured null are data, not evidence gaps"
    );
    assert_eq!(outcome.value_evaluations.unavailable, 0);
}

#[tokio::test]
async fn unavailable_evidence_is_typed_and_makes_the_result_incomplete() {
    let fx = fixture(
        vec![
            CallRow {
                run_id: "r1",
                call_id: 1,
                status: "succeeded",
                args_state: "redacted",
                args_handle: Some(b"h-redacted"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 2,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-ok"),
                ..CallRow::default()
            },
        ],
        FixtureResolver {
            values: vec![(b"h-ok", customer(35))],
            unavailable: vec![(b"h-redacted", UnavailableReason::Redacted)],
            ..FixtureResolver::default()
        },
    );
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM retained_calls_v1 \
             WHERE args['customer']['age'] >= 30",
        )
        .await
        .expect("plans");
    assert_eq!(
        call_ids(&mut run).await,
        vec![2],
        "undecidable rows leave the stream"
    );
    let outcome = run.finish();
    assert_eq!(outcome.result_state, ResultState::Incomplete);
    assert_eq!(outcome.value_evaluations.unavailable, 1);
    assert_eq!(
        outcome.value_evaluations.by_reason.get("redacted"),
        Some(&1),
        "the reason is reconciled in the outcome"
    );
}

// ── whole-value equality (D7) ──────────────────────────────────────────

#[tokio::test]
async fn whole_value_equality_is_semantic_never_handle_bytes() {
    // Two DIFFERENT handles resolve to semantically equal values (one
    // with reordered map entries). Handle-byte equality would find one
    // row; canonical semantic equality finds both.
    let forward = CanonValue::Map(vec![
        ("a".to_string(), CanonValue::Int(1)),
        ("b".to_string(), CanonValue::Int(2)),
    ]);
    let reversed = CanonValue::Map(vec![
        ("b".to_string(), CanonValue::Int(2)),
        ("a".to_string(), CanonValue::Int(1)),
    ]);
    let fx = fixture(
        vec![
            CallRow {
                run_id: "r1",
                call_id: 1,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"handle-A"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 2,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"handle-B-different-bytes"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 3,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"handle-C"),
                ..CallRow::default()
            },
        ],
        FixtureResolver {
            values: vec![
                (b"handle-A", forward),
                (b"handle-B-different-bytes", reversed),
                (b"handle-C", customer(1)),
            ],
            ..FixtureResolver::default()
        },
    );
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM retained_calls_v1 \
             WHERE args = baml_value_json('{\"a\":1,\"b\":2}')",
        )
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, vec![1, 2]);
    assert_eq!(run.finish().result_state, ResultState::Complete);
}

#[tokio::test]
async fn cid_equality_uses_canonical_identity_not_storage_identity() {
    // The reference CID equals the canonical encoding of `customer(35)`.
    let target = customer(35);
    let encoded = canon::encode(&target);
    let cid_wire = canon::cid_wire(&encoded.root_cid);
    let fx = fixture(
        vec![
            // Backend proves this handle's canonical CID without decode.
            CallRow {
                run_id: "r1",
                call_id: 1,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-proved"),
                ..CallRow::default()
            },
            // This one must hydrate + compare semantically.
            CallRow {
                run_id: "r1",
                call_id: 2,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-hydrated"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 3,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-other"),
                ..CallRow::default()
            },
        ],
        FixtureResolver {
            values: vec![
                (b"h-proved", target.clone()),
                (b"h-hydrated", target.clone()),
                (b"h-other", customer(99)),
            ],
            cids: vec![(b"h-proved", encoded.root_cid)],
            cid_bodies: vec![(encoded.root_cid, target)],
            ..FixtureResolver::default()
        },
    );
    let mut run = fx
        .session
        .execute(&format!(
            "SELECT call_id FROM retained_calls_v1 \
             WHERE args = baml_value_cid('{cid_wire}')"
        ))
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, vec![1, 2]);
}

// ── rendering ──────────────────────────────────────────────────────────

#[tokio::test]
async fn selected_values_render_and_unavailable_selection_is_incomplete() {
    use datafusion::arrow::array::StringArray;
    let fx = fixture(
        vec![
            CallRow {
                run_id: "r1",
                call_id: 1,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-ok"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 2,
                status: "lost",
                args_state: "lost",
                args_handle: Some(b"h-lost"),
                ..CallRow::default()
            },
        ],
        FixtureResolver {
            values: vec![(b"h-ok", customer(35))],
            unavailable: vec![(b"h-lost", UnavailableReason::Lost)],
            ..FixtureResolver::default()
        },
    );
    let mut run = fx
        .session
        .execute("SELECT call_id, args FROM retained_calls_v1 ORDER BY call_id")
        .await
        .expect("plans");
    let mut rendered = Vec::new();
    while let Some(batch) = run.next_batch().await {
        let col = batch
            .column_by_name("args")
            .expect("args column")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("rendered as text")
            .clone();
        for i in 0..col.len() {
            rendered.push(
                col.is_null(i)
                    .then_some(None)
                    .unwrap_or(Some(col.value(i).to_string())),
            );
        }
    }
    assert_eq!(
        rendered,
        vec![
            Some(r#"{"customer":{"age":35,"name":"ada"}}"#.to_string()),
            None,
        ]
    );
    let outcome = run.finish();
    assert_eq!(outcome.result_state, ResultState::Incomplete);
    assert_eq!(outcome.value_evaluations.by_reason.get("lost"), Some(&1));
}

// ── forbidden constructs ───────────────────────────────────────────────

#[tokio::test]
async fn forbidden_constructs_fail_closed() {
    let fx = fixture(vec![], FixtureResolver::default());
    for (sql, needle) in [
        ("CREATE FUNCTION f AS 'x'", "CREATE FUNCTION"),
        ("INSERT INTO runs_v1 VALUES (1)", "only queries"),
        ("DROP TABLE runs_v1", "only queries"),
        ("SELECT 1; SELECT 2", "exactly one statement"),
        (
            "SELECT __baml_path(args, '[]', 'args') FROM retained_calls_v1",
            "reserved",
        ),
    ] {
        let err = fx.session.execute(sql).await.err().expect(sql).0;
        assert_eq!(err.code, QueryErrorCode::InvalidSql, "{sql}");
        assert!(
            err.message.contains(needle),
            "{sql}: expected {needle:?} in {:?}",
            err.message
        );
    }
}

// ── backend capability (D4) ────────────────────────────────────────────

#[tokio::test]
async fn backend_gated_function_fails_at_planning_before_any_read() {
    let mut capabilities = CapabilityRegistry::new();
    capabilities.require("clickhouse_quantile_exact", Backend::Clickhouse);
    let fx = fixture_with(
        vec![CallRow {
            run_id: "r1",
            call_id: 1,
            status: "succeeded",
            args_state: "available",
            args_handle: None,
            ..CallRow::default()
        }],
        FixtureResolver::default(),
        QueryBudgets::unlimited(),
        CancellationToken::new(),
        capabilities,
        QueryScope::local(),
    );
    let err = fx
        .session
        .execute("SELECT clickhouse_quantile_exact(duration_ns) FROM retained_calls_v1")
        .await
        .err()
        .expect("gated function rejected")
        .0;
    assert_eq!(err.code, QueryErrorCode::BackendCapability);
    assert_eq!(err.code.as_str(), "E_BACKEND_CAPABILITY");
    assert!(err.message.contains("required_backend: clickhouse"));
    assert!(err.message.contains("current_backend: local"));
    assert!(
        fx.scans.lock().unwrap().is_empty(),
        "the capability failure precedes every provider read"
    );
}

// ── budgets and cancellation ───────────────────────────────────────────

#[tokio::test]
async fn result_row_budget_is_query_global_and_terminal() {
    let rows: Vec<CallRow> = (0..100)
        .map(|i| CallRow {
            run_id: "r1",
            call_id: i,
            status: "succeeded",
            args_state: "available",
            args_handle: None,
            ..CallRow::default()
        })
        .collect();
    let mut budgets = QueryBudgets::unlimited();
    budgets.max_result_rows = 10;
    let fx = fixture_with(
        rows,
        FixtureResolver::default(),
        budgets,
        CancellationToken::new(),
        CapabilityRegistry::new(),
        QueryScope::local(),
    );
    let mut run = fx
        .session
        .execute("SELECT call_id FROM retained_calls_v1")
        .await
        .expect("plans");
    let _ = call_ids(&mut run).await;
    let outcome = run.finish();
    assert_eq!(outcome.result_state, ResultState::BudgetExhausted);
    assert!(!outcome.query_completed);
    let error = outcome.error.expect("typed budget error");
    assert_eq!(error.code, "E_QUERY_BUDGET_EXCEEDED");
}

#[tokio::test]
async fn cancellation_produces_a_typed_terminal_outcome() {
    let cancel = CancellationToken::new();
    let fx = fixture_with(
        vec![CallRow {
            run_id: "r1",
            call_id: 1,
            status: "succeeded",
            args_state: "available",
            args_handle: None,
            ..CallRow::default()
        }],
        FixtureResolver::default(),
        QueryBudgets::unlimited(),
        cancel.clone(),
        CapabilityRegistry::new(),
        QueryScope::local(),
    );
    let mut run = fx
        .session
        .execute("SELECT call_id FROM retained_calls_v1")
        .await
        .expect("plans");
    cancel.cancel();
    assert!(run.next_batch().await.is_none(), "cancelled before batches");
    let outcome = run.finish();
    assert_eq!(outcome.result_state, ResultState::Cancelled);
    assert!(!outcome.query_completed);
}

// ── authorization ──────────────────────────────────────────────────────

#[tokio::test]
async fn value_reads_fail_before_execution_without_the_right() {
    let mut scope = QueryScope::local();
    scope.value_read_allowed = false;
    let fx = fixture_with(
        vec![],
        FixtureResolver::default(),
        QueryBudgets::unlimited(),
        CancellationToken::new(),
        CapabilityRegistry::new(),
        scope,
    );
    // Resident-only queries stay allowed.
    assert!(
        fx.session
            .execute("SELECT call_id FROM retained_calls_v1")
            .await
            .is_ok()
    );
    fx.scans.lock().unwrap().clear();
    let err = fx
        .session
        .execute("SELECT call_id FROM retained_calls_v1 WHERE args['x'] = 1")
        .await
        .err()
        .expect("value read denied")
        .0;
    assert_eq!(err.code, QueryErrorCode::AuthorizationDenied);
    assert!(
        fx.scans.lock().unwrap().is_empty(),
        "denied before any read"
    );
}

// ── final limit vs value predicates ────────────────────────────────────

#[tokio::test]
async fn final_limit_never_reaches_the_provider_below_a_value_predicate() {
    let rows: Vec<CallRow> = (0..10)
        .map(|i| CallRow {
            run_id: "r1",
            call_id: i,
            status: "succeeded",
            args_state: "available",
            args_handle: Some(if i % 2 == 0 { b"h-even" } else { b"h-odd" }),
            ..CallRow::default()
        })
        .collect();
    let fx = fixture(
        rows,
        FixtureResolver {
            values: vec![(b"h-even", customer(50)), (b"h-odd", customer(20))],
            ..FixtureResolver::default()
        },
    );
    // Value-predicate query with LIMIT: the provider must see NO limit —
    // the limit applies only after the exact residual predicate.
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM retained_calls_v1 \
             WHERE args['customer']['age'] >= 30 LIMIT 3",
        )
        .await
        .expect("plans");
    let ids = call_ids(&mut run).await;
    assert_eq!(ids.len(), 3);
    assert!(
        ids.iter().all(|id| id % 2 == 0),
        "only matching rows: {ids:?}"
    );
    let scans = fx.scans.lock().unwrap().clone();
    assert!(
        scans.iter().all(Option::is_none),
        "a final limit must never push below a value predicate: {scans:?}"
    );

    // Control: a resident-only limit MAY reach the provider.
    fx.scans.lock().unwrap().clear();
    let mut run = fx
        .session
        .execute("SELECT call_id FROM retained_calls_v1 LIMIT 3")
        .await
        .expect("plans");
    let ids = call_ids(&mut run).await;
    assert_eq!(ids.len(), 3);
    let scans = fx.scans.lock().unwrap().clone();
    assert!(
        scans.iter().any(|l| *l == Some(3)),
        "resident-only limits push down: {scans:?}"
    );
}

// ── aliases and snapshot ───────────────────────────────────────────────

#[tokio::test]
async fn unversioned_alias_is_pinned_to_the_bound_catalog_version() {
    let fx = fixture(
        vec![CallRow {
            run_id: "r1",
            call_id: 7,
            status: "succeeded",
            args_state: "available",
            args_handle: None,
            ..CallRow::default()
        }],
        FixtureResolver::default(),
    );
    for table in ["retained_calls_v1", "retained_calls"] {
        let mut run = fx
            .session
            .execute(&format!("SELECT call_id FROM {table}"))
            .await
            .expect("plans");
        assert_eq!(call_ids(&mut run).await, vec![7], "{table}");
        let outcome = run.finish();
        assert_eq!(outcome.snapshot.catalog_version, "v1");
        assert_eq!(outcome.snapshot.generation, "fixture-1");
    }
}

// ── dependency hygiene (Q1 gate: backend-neutral core) ─────────────────

/// The core must not depend on the CLI, runtime host, AWS SDK, concrete
/// SQLite, or concrete ClickHouse client — pinned against the manifest.
#[test]
fn core_dependencies_stay_backend_neutral() {
    let manifest = include_str!("../Cargo.toml");
    let allowed = [
        "bex_events",
        "async-trait",
        "datafusion",
        "futures",
        "serde",
        "serde_json",
        "tokio",
    ];
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line == "[dependencies]";
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name = line.split(['=', ' ']).next().unwrap_or_default();
        assert!(
            allowed.contains(&name),
            "unexpected core dependency {name:?}: the backend-neutral \
             contract forbids CLI/host/AWS/SQLite/ClickHouse coupling; \
             extend the allowlist only with a deliberate decision"
        );
    }
    for forbidden in ["baml_cli", "bex_engine", "aws", "rusqlite", "clickhouse"] {
        assert!(
            !manifest.contains(forbidden),
            "forbidden dependency family {forbidden:?} in the core manifest"
        );
    }
}

/// Regression: a QUALIFIED bare value column (`c.args`) in a projection
/// with a parent node referencing it (ORDER BY) must keep its qualifier
/// through the rendering rewrite.
#[tokio::test]
async fn qualified_value_columns_survive_projection_rewrite() {
    let fx = fixture(
        vec![
            CallRow {
                run_id: "r1",
                call_id: 2,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-b"),
                ..CallRow::default()
            },
            CallRow {
                run_id: "r1",
                call_id: 1,
                status: "succeeded",
                args_state: "available",
                args_handle: Some(b"h-a"),
                ..CallRow::default()
            },
        ],
        FixtureResolver {
            values: vec![(b"h-a", CanonValue::Int(1)), (b"h-b", CanonValue::Int(2))],
            ..FixtureResolver::default()
        },
    );
    let mut run = fx
        .session
        .execute("SELECT c.call_id, c.args FROM retained_calls_v1 c ORDER BY c.call_id")
        .await
        .expect("qualified value projection plans");
    use datafusion::arrow::array::StringArray;
    let mut rendered = Vec::new();
    while let Some(batch) = run.next_batch().await {
        let col = batch
            .column_by_name("args")
            .expect("args column")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("rendered as text")
            .clone();
        for i in 0..col.len() {
            rendered.push(col.value(i).to_string());
        }
    }
    assert_eq!(rendered, vec!["1", "2"]);
    assert_eq!(run.finish().result_state, ResultState::Complete);
}
