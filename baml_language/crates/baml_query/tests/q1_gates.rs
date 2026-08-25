//! Q1 freeze-gate tests (cct-1 gate suite ported onto catalog v1,
//! TASK/baml-query-scope.md §8 Phase 1).
//!
//! These pin the public contract against an in-memory fixture backend:
//! natural value expressions and their internal rewrites; whole-value
//! equality that is never handle-byte equality; typed unavailability and
//! outcome reconciliation; forbidden constructs; planning-time backend
//! capability failures; query-global budgets and cancellation; the
//! final-limit rule (a limit never reaches the provider when a value
//! predicate is in play); catalog views and discovery; and the
//! dependency allowlist.

use std::sync::{Arc, Mutex};

use baml_query::{
    budget::{CancellationToken, QueryBudgets},
    capability::CapabilityRegistry,
    catalog::{CatalogProfile, RelationDef},
    error::QueryErrorCode,
    outcome::{ResultState, UnavailableReason},
    provider::RelationProviderFactory,
    scope::{Backend, QueryScope, Snapshot},
    session::QuerySessionBuilder,
    value::{
        model::Value,
        resolver::{DecodeCaps, Resolved, ValueResolver},
    },
};
use datafusion::{
    arrow::{
        array::{
            Array as _, ArrayRef, BinaryBuilder, BooleanBuilder, ListBuilder, StringBuilder,
            TimestampNanosecondBuilder, UInt32Builder, UInt64Builder,
        },
        record_batch::RecordBatch,
    },
    catalog::Session,
    common::Result as DfResult,
    datasource::{TableProvider, TableType, memory::MemTable},
    logical_expr::{Expr, TableProviderFilterPushDown},
    physical_plan::ExecutionPlan,
};

// ── fixture backend ────────────────────────────────────────────────────

/// One retained-call fixture row (only the fields the tests vary).
#[derive(Clone, Default)]
struct CallRow {
    execution_id: &'static str,
    call_id: String,
    definition_key: Option<&'static str>,
    status: Option<&'static str>,
    kind: Option<&'static str>,
    args_state: &'static str,
    /// Handle bytes for the `args` role (None = role not applicable).
    args_handle: Option<&'static [u8]>,
}

fn call(execution_id: &'static str, call_id: impl Into<String>) -> CallRow {
    CallRow {
        execution_id,
        call_id: call_id.into(),
        status: Some("ok"),
        args_state: "available",
        ..CallRow::default()
    }
}

#[expect(clippy::too_many_lines, reason = "one arm per catalog column")]
fn calls_batch(relation: &RelationDef, rows: &[CallRow]) -> RecordBatch {
    let schema = relation.schema();
    let mut arrays: Vec<ArrayRef> = Vec::new();
    for column in &relation.columns {
        let array: ArrayRef = match column.name {
            "execution_id" => {
                let mut b = StringBuilder::new();
                for r in rows {
                    b.append_value(r.execution_id);
                }
                Arc::new(b.finish())
            }
            "call_id" => {
                let mut b = StringBuilder::new();
                for r in rows {
                    b.append_value(&r.call_id);
                }
                Arc::new(b.finish())
            }
            "thread_id" => {
                let mut b = StringBuilder::new();
                for _ in rows {
                    b.append_value("t1");
                }
                Arc::new(b.finish())
            }
            "parent_call_id"
            | "call_path_id"
            | "call_path_overflow_reason"
            | "fqn"
            | "call_site_file"
            | "args_cid"
            | "output_cid"
            | "error_cid"
            | "error_id"
            | "error_lost_reason" => {
                let mut b = StringBuilder::new();
                for _ in rows {
                    b.append_null();
                }
                Arc::new(b.finish())
            }
            "definition_key" => {
                let mut b = StringBuilder::new();
                for r in rows {
                    b.append_option(r.definition_key);
                }
                Arc::new(b.finish())
            }
            "kind" => {
                let mut b = StringBuilder::new();
                for r in rows {
                    b.append_option(r.kind);
                }
                Arc::new(b.finish())
            }
            "function_id" => {
                let mut b = UInt32Builder::new();
                for _ in rows {
                    b.append_value(1);
                }
                Arc::new(b.finish())
            }
            "call_site_line" | "call_site_start" | "call_site_end" => {
                let mut b = UInt32Builder::new();
                for _ in rows {
                    b.append_null();
                }
                Arc::new(b.finish())
            }
            "edge_kind" => {
                let mut b = StringBuilder::new();
                for _ in rows {
                    b.append_value("call");
                }
                Arc::new(b.finish())
            }
            "started_ns" => {
                let mut b = UInt64Builder::new();
                for _ in rows {
                    b.append_value(10);
                }
                Arc::new(b.finish())
            }
            "ended_ns" | "duration_ns" => {
                let mut b = UInt64Builder::new();
                for _ in rows {
                    b.append_null();
                }
                Arc::new(b.finish())
            }
            "started_at" | "ended_at" => {
                let mut b = TimestampNanosecondBuilder::new().with_timezone("UTC");
                for _ in rows {
                    b.append_null();
                }
                Arc::new(b.finish())
            }
            "status" => {
                let mut b = StringBuilder::new();
                for r in rows {
                    b.append_option(r.status);
                }
                Arc::new(b.finish())
            }
            "selection_reasons" | "roles" | "runtime_ids" => {
                let mut b = ListBuilder::new(StringBuilder::new()).with_field(
                    datafusion::arrow::datatypes::Field::new(
                        "item",
                        datafusion::arrow::datatypes::DataType::Utf8,
                        false,
                    ),
                );
                for _ in rows {
                    b.append(true);
                }
                Arc::new(b.finish())
            }
            "args_state" => {
                let mut b = StringBuilder::new();
                for r in rows {
                    b.append_value(r.args_state);
                }
                Arc::new(b.finish())
            }
            "output_state" | "error_state" => {
                let mut b = StringBuilder::new();
                for _ in rows {
                    b.append_value("not_applicable");
                }
                Arc::new(b.finish())
            }
            "args" => {
                let mut b = BinaryBuilder::new();
                for r in rows {
                    b.append_option(r.args_handle);
                }
                Arc::new(b.finish())
            }
            "output" | "error" => {
                let mut b = BinaryBuilder::new();
                for _ in rows {
                    b.append_null();
                }
                Arc::new(b.finish())
            }
            other => panic!("fixture does not know column {other}"),
        };
        arrays.push(array);
    }
    let _ = BooleanBuilder::new();
    RecordBatch::try_new(schema, arrays).expect("fixture batch matches catalog schema")
}

/// Fixture resolver: handle bytes → values or typed gaps, batched.
#[derive(Default)]
struct FixtureResolver {
    values: Vec<(&'static [u8], Value)>,
    unavailable: Vec<(&'static [u8], UnavailableReason)>,
    /// Handles whose canonical CID the backend can prove without decode.
    cids: Vec<(&'static [u8], [u8; 32])>,
    cid_bodies: Vec<([u8; 32], Value)>,
}

impl ValueResolver for FixtureResolver {
    fn resolve_many(&self, handles: &[&[u8]], _caps: DecodeCaps) -> Vec<Resolved> {
        handles
            .iter()
            .map(|handle| {
                if let Some((_, reason)) = self.unavailable.iter().find(|(h, _)| h == handle) {
                    return Resolved::Unavailable(*reason);
                }
                match self.values.iter().find(|(h, _)| h == handle) {
                    Some((_, value)) => Resolved::Value(Arc::new(value.clone())),
                    None => Resolved::Unavailable(UnavailableReason::Lost),
                }
            })
            .collect()
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
        if relation.name != "calls_v1" {
            return Ok(None);
        }
        let batch = calls_batch(relation, &self.calls);
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

async fn fixture(calls: Vec<CallRow>, resolver: FixtureResolver) -> Fixture {
    fixture_with(
        calls,
        resolver,
        QueryBudgets::unlimited(),
        CancellationToken::new(),
        CapabilityRegistry::new(),
        QueryScope::local(),
    )
    .await
}

async fn fixture_with(
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
        CatalogProfile::public(),
        scope,
        snapshot(),
        Arc::new(resolver),
        factory,
    )
    .with_budgets(budgets)
    .with_cancellation(cancel)
    .with_capabilities(capabilities)
    .build()
    .await
    .expect("fixture session builds");
    Fixture { session, scans }
}

fn customer(age: i64) -> Value {
    Value::Map(vec![(
        "customer".to_string(),
        Value::Map(vec![
            ("age".to_string(), Value::Int(age)),
            ("name".to_string(), Value::String("ada".to_string())),
        ]),
    )])
}

/// Collect the `call_id` column (Utf8 wire strings) across all batches.
async fn call_ids(execution: &mut baml_query::QueryExecution) -> Vec<String> {
    use datafusion::arrow::array::StringArray;
    let mut out = Vec::new();
    while let Some(batch) = execution.next_batch().await {
        let col = batch
            .column_by_name("call_id")
            .expect("call_id column")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("utf8")
            .clone();
        out.extend(col.iter().flatten().map(str::to_string));
    }
    out
}

// ── natural value expressions ──────────────────────────────────────────

#[tokio::test]
async fn nested_subscript_comparison_filters_by_canonical_semantics() {
    let fx = fixture(
        vec![
            CallRow {
                args_handle: Some(b"h-young"),
                ..call("e1", "c1")
            },
            CallRow {
                args_handle: Some(b"h-old"),
                ..call("e1", "c2")
            },
        ],
        FixtureResolver {
            values: vec![(b"h-young", customer(22)), (b"h-old", customer(41))],
            ..FixtureResolver::default()
        },
    )
    .await;
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM calls_v1 \
             WHERE args['customer']['age'] >= 30",
        )
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, vec!["c2"]);
    let outcome = run.finish();
    assert_eq!(outcome.result_state, ResultState::Complete);
    assert_eq!(outcome.value_evaluations.attempted, 2);
    assert_eq!(outcome.value_evaluations.available, 2);
}

#[tokio::test]
async fn args_root_is_a_named_argument_object() {
    let fx = fixture(vec![], FixtureResolver::default()).await;
    let err = fx
        .session
        .execute("SELECT call_id FROM calls_v1 WHERE args[0] = 1")
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
                args_handle: Some(b"h-nokey"),
                ..call("e1", "c1")
            },
            CallRow {
                args_handle: Some(b"h-null"),
                ..call("e1", "c2")
            },
            CallRow {
                args_handle: Some(b"h-match"),
                ..call("e1", "c3")
            },
        ],
        FixtureResolver {
            values: vec![
                (b"h-nokey", Value::Map(vec![])),
                (
                    b"h-null",
                    Value::Map(vec![(
                        "customer".to_string(),
                        Value::Map(vec![("age".to_string(), Value::Null)]),
                    )]),
                ),
                (b"h-match", customer(50)),
            ],
            ..FixtureResolver::default()
        },
    )
    .await;
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM calls_v1 \
             WHERE args['customer']['age'] >= 30",
        )
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, vec!["c3"]);
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
                args_state: "lost:value_redacted",
                args_handle: Some(b"h-redacted"),
                ..call("e1", "c1")
            },
            CallRow {
                args_handle: Some(b"h-ok"),
                ..call("e1", "c2")
            },
        ],
        FixtureResolver {
            values: vec![(b"h-ok", customer(35))],
            unavailable: vec![(b"h-redacted", UnavailableReason::Redacted)],
            ..FixtureResolver::default()
        },
    )
    .await;
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM calls_v1 \
             WHERE args['customer']['age'] >= 30",
        )
        .await
        .expect("plans");
    assert_eq!(
        call_ids(&mut run).await,
        vec!["c2"],
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

// ── whole-value equality ───────────────────────────────────────────────

#[tokio::test]
async fn whole_value_equality_is_semantic_never_handle_bytes() {
    // Two DIFFERENT handles resolve to semantically equal values (one
    // with reordered map entries). Handle-byte equality would find one
    // row; canonical semantic equality finds both.
    let forward = Value::Map(vec![
        ("a".to_string(), Value::Int(1)),
        ("b".to_string(), Value::Int(2)),
    ]);
    let reversed = Value::Map(vec![
        ("b".to_string(), Value::Int(2)),
        ("a".to_string(), Value::Int(1)),
    ]);
    let fx = fixture(
        vec![
            CallRow {
                args_handle: Some(b"handle-A"),
                ..call("e1", "c1")
            },
            CallRow {
                args_handle: Some(b"handle-B-different-bytes"),
                ..call("e1", "c2")
            },
            CallRow {
                args_handle: Some(b"handle-C"),
                ..call("e1", "c3")
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
    )
    .await;
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM calls_v1 \
             WHERE args = baml_value_json('{\"a\":1,\"b\":2}')",
        )
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, vec!["c1", "c2"]);
    assert_eq!(run.finish().result_state, ResultState::Complete);
}

#[tokio::test]
async fn cid_equality_uses_canonical_identity_not_storage_identity() {
    let target = customer(35);
    let cid: [u8; 32] = [0xAB; 32];
    let cid_wire = format!("bamlv_1_{}", "ab".repeat(32));
    let fx = fixture(
        vec![
            // Backend proves this handle's canonical CID without decode.
            CallRow {
                args_handle: Some(b"h-proved"),
                ..call("e1", "c1")
            },
            // This one must hydrate + compare semantically.
            CallRow {
                args_handle: Some(b"h-hydrated"),
                ..call("e1", "c2")
            },
            CallRow {
                args_handle: Some(b"h-other"),
                ..call("e1", "c3")
            },
        ],
        FixtureResolver {
            values: vec![
                (b"h-proved", target.clone()),
                (b"h-hydrated", target.clone()),
                (b"h-other", customer(99)),
            ],
            cids: vec![(b"h-proved", cid)],
            cid_bodies: vec![(cid, target)],
            ..FixtureResolver::default()
        },
    )
    .await;
    let mut run = fx
        .session
        .execute(&format!(
            "SELECT call_id FROM calls_v1 \
             WHERE args = baml_value_cid('{cid_wire}')"
        ))
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, vec!["c1", "c2"]);
}

// ── rendering ──────────────────────────────────────────────────────────

#[tokio::test]
async fn selected_values_render_and_unavailable_selection_is_incomplete() {
    use datafusion::arrow::array::StringArray;
    let fx = fixture(
        vec![
            CallRow {
                args_handle: Some(b"h-ok"),
                ..call("e1", "c1")
            },
            CallRow {
                args_state: "lost:transport",
                args_handle: Some(b"h-lost"),
                ..call("e1", "c2")
            },
        ],
        FixtureResolver {
            values: vec![(b"h-ok", customer(35))],
            unavailable: vec![(b"h-lost", UnavailableReason::Lost)],
            ..FixtureResolver::default()
        },
    )
    .await;
    let mut run = fx
        .session
        .execute("SELECT call_id, args FROM calls_v1 ORDER BY call_id")
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
            rendered.push(if col.is_null(i) {
                None
            } else {
                Some(col.value(i).to_string())
            });
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
    let fx = fixture(vec![], FixtureResolver::default()).await;
    for (sql, needle) in [
        ("CREATE FUNCTION f AS 'x'", "CREATE FUNCTION"),
        ("INSERT INTO calls_v1 VALUES (1)", "only queries"),
        ("DROP TABLE calls_v1", "only queries"),
        ("SELECT 1; SELECT 2", "exactly one statement"),
        (
            "SELECT __baml_path(args, '[]', 'input') FROM calls_v1",
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

// ── discovery ──────────────────────────────────────────────────────────

#[tokio::test]
async fn views_plan_and_discovery_statements_work() {
    let fx = fixture(
        vec![CallRow {
            kind: Some("bytecode"),
            ..call("e1", "c1")
        }],
        FixtureResolver::default(),
    )
    .await;
    // Every shipped view plans; llm_calls stays out of the catalog until a
    // producer emits an LLM function kind (it would only ever be empty).
    let mut run = fx
        .session
        .execute("SELECT call_path_id FROM hot_call_paths LIMIT 1")
        .await
        .expect("hot_call_paths view plans over an empty provider");
    while run.next_batch().await.is_some() {}
    let err = fx
        .session
        .execute("SELECT call_id FROM llm_calls")
        .await
        .err()
        .expect("llm_calls does not exist in catalog v1")
        .0;
    assert_eq!(err.code, QueryErrorCode::InvalidSql);
    // Discovery statements pass the gatekeeper and execute.
    for sql in ["SHOW TABLES", "SHOW COLUMNS FROM calls", "DESCRIBE threads"] {
        let mut run = fx.session.execute(sql).await.unwrap_or_else(|(e, _)| {
            panic!("{sql}: {e}");
        });
        assert!(
            run.next_batch().await.is_some(),
            "{sql} returns at least one batch"
        );
    }
}

#[tokio::test]
async fn baml_columns_documents_the_profile_in_sql() {
    use datafusion::arrow::array::StringArray;
    let fx = fixture(vec![], FixtureResolver::default()).await;
    let mut run = fx
        .session
        .execute(
            r#"SELECT doc FROM baml_columns WHERE relation = 'calls_v1' AND "column" = 'args'"#,
        )
        .await
        .expect("baml_columns plans");
    let mut docs = Vec::new();
    while let Some(batch) = run.next_batch().await {
        let col = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("utf8 docs")
            .clone();
        docs.extend(col.iter().flatten().map(str::to_string));
    }
    assert_eq!(docs.len(), 1);
    assert!(docs[0].contains("named-argument object"), "{docs:?}");
}

#[tokio::test]
async fn count_star_plans_with_the_default_planners_intact() {
    // Regression: registering the BAML value planner must APPEND to the
    // default expr planners, not replace them — losing the aggregate
    // planner left `COUNT(*)` as an unplannable wildcard.
    use datafusion::arrow::array::Int64Array;
    let fx = fixture(
        vec![call("e1", "c1"), call("e1", "c2")],
        FixtureResolver::default(),
    )
    .await;
    let mut run = fx
        .session
        .execute("SELECT count(*) FROM calls_v1")
        .await
        .expect("count(*) plans");
    let batch = run.next_batch().await.expect("one batch");
    let count = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("count column")
        .value(0);
    assert_eq!(count, 2);
    assert_eq!(run.finish().result_state, ResultState::Complete);
}

/// The bug class behind the COUNT(*) regression: session assembly that
/// silently drops `DataFusion` defaults (planners, functions, analyzer
/// rules). Every statement here exercises a different default-owned
/// planning path; each must plan AND stream to a complete outcome.
#[tokio::test]
async fn standard_sql_surface_survives_the_custom_session_assembly() {
    let fx = fixture(
        vec![
            CallRow {
                definition_key: Some("k.a"),
                ..call("e1", "c1")
            },
            CallRow {
                definition_key: Some("k.b"),
                ..call("e1", "c2")
            },
            CallRow {
                definition_key: Some("k.a"),
                ..call("e2", "c3")
            },
        ],
        FixtureResolver::default(),
    )
    .await;
    for sql in [
        // Aggregates (default aggregate planner rewrites COUNT(*)).
        "SELECT count(*), count(DISTINCT execution_id) FROM calls_v1",
        "SELECT count(*) FROM hot_call_paths",
        "SELECT execution_id, count(*) AS n FROM calls GROUP BY execution_id ORDER BY n DESC",
        // Joins + aggregates together.
        "SELECT a.execution_id, count(*) FROM calls a \
         JOIN calls b ON a.execution_id = b.execution_id GROUP BY a.execution_id",
        // CTEs, subqueries, set ops.
        "WITH mine AS (SELECT call_id FROM calls WHERE execution_id = 'e1') \
         SELECT count(*) FROM mine",
        "SELECT call_id FROM calls WHERE execution_id IN \
         (SELECT execution_id FROM threads UNION ALL SELECT 'e1')",
        // Window functions.
        "SELECT call_id, row_number() OVER (PARTITION BY execution_id \
         ORDER BY call_id) AS rn FROM calls",
        // Scalar functions, CASE, LIKE (default function registry).
        "SELECT upper(call_id), length(execution_id), \
         CASE WHEN status = 'ok' THEN 1 ELSE 0 END FROM calls \
         WHERE call_id LIKE 'c%'",
        // Subscripts on a RESIDENT list column must fall through our
        // field-access planner to the default one.
        "SELECT selection_reasons[1] FROM calls",
        // information_schema behind SHOW support.
        "SELECT count(*) FROM information_schema.tables",
    ] {
        let mut run = fx
            .session
            .execute(sql)
            .await
            .unwrap_or_else(|(e, _)| panic!("{sql}: {e}"));
        while run.next_batch().await.is_some() {}
        let outcome = run.finish();
        assert_eq!(
            outcome.result_state,
            ResultState::Complete,
            "{sql}: {:?}",
            outcome.error
        );
    }
}

#[tokio::test]
async fn unknown_table_gets_a_did_you_mean_remedy() {
    let fx = fixture(vec![], FixtureResolver::default()).await;
    let err = fx
        .session
        .execute("SELECT * FROM thread")
        .await
        .err()
        .expect("unknown table fails")
        .0;
    assert_eq!(err.code, QueryErrorCode::InvalidSql, "msg={}", err.message);
    assert_eq!(err.remedy.as_deref(), Some("did you mean `threads`?"));
}

// ── backend capability ─────────────────────────────────────────────────

#[tokio::test]
async fn backend_gated_function_fails_at_planning_before_any_read() {
    let mut capabilities = CapabilityRegistry::new();
    capabilities.require("clickhouse_quantile_exact", Backend::Clickhouse);
    let fx = fixture_with(
        vec![call("e1", "c1")],
        FixtureResolver::default(),
        QueryBudgets::unlimited(),
        CancellationToken::new(),
        capabilities,
        QueryScope::local(),
    )
    .await;
    let err = fx
        .session
        .execute("SELECT clickhouse_quantile_exact(duration_ns) FROM calls_v1")
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
    let rows: Vec<CallRow> = (0..100).map(|i| call("e1", format!("c{i:03}"))).collect();
    let mut budgets = QueryBudgets::unlimited();
    budgets.max_result_rows = 10;
    let fx = fixture_with(
        rows,
        FixtureResolver::default(),
        budgets,
        CancellationToken::new(),
        CapabilityRegistry::new(),
        QueryScope::local(),
    )
    .await;
    let mut run = fx
        .session
        .execute("SELECT call_id FROM calls_v1")
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
        vec![call("e1", "c1")],
        FixtureResolver::default(),
        QueryBudgets::unlimited(),
        cancel.clone(),
        CapabilityRegistry::new(),
        QueryScope::local(),
    )
    .await;
    let mut run = fx
        .session
        .execute("SELECT call_id FROM calls_v1")
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
    )
    .await;
    // Resident-only queries stay allowed.
    assert!(
        fx.session
            .execute("SELECT call_id FROM calls_v1")
            .await
            .is_ok()
    );
    fx.scans.lock().unwrap().clear();
    let err = fx
        .session
        .execute("SELECT call_id FROM calls_v1 WHERE args['x'] = 1")
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

#[tokio::test]
async fn value_reads_inside_subqueries_are_denied_without_the_right() {
    // Regression: the authorization/capability walks once used the plain
    // `apply` family, which skips the plans embedded in EXISTS/IN/scalar
    // subqueries — a bypass for both gates.
    let mut scope = QueryScope::local();
    scope.value_read_allowed = false;
    let fx = fixture_with(
        vec![call("e1", "c1")],
        FixtureResolver::default(),
        QueryBudgets::unlimited(),
        CancellationToken::new(),
        CapabilityRegistry::new(),
        scope,
    )
    .await;
    for sql in [
        "SELECT call_id FROM calls_v1 WHERE call_id IN \
         (SELECT call_id FROM calls_v1 WHERE args['x'] = 1)",
        "SELECT call_id FROM calls_v1 WHERE EXISTS \
         (SELECT 1 FROM calls_v1 WHERE args['x'] = 1)",
        "SELECT call_id FROM calls_v1 WHERE call_id = \
         (SELECT max(call_id) FROM calls_v1 WHERE args['x'] = 1)",
    ] {
        let err = fx
            .session
            .execute(sql)
            .await
            .err()
            .unwrap_or_else(|| panic!("{sql}: subquery value read must be denied"))
            .0;
        assert_eq!(err.code, QueryErrorCode::AuthorizationDenied, "{sql}");
    }
}

#[tokio::test]
async fn derived_table_value_columns_stay_handles_for_outer_predicates() {
    // Regression: the bare-value render rewrite once ran over EVERY
    // projection, turning an interior derived table's Binary handle into
    // rendered text underneath an already-lowered outer comparison.
    let fx = fixture(
        vec![
            CallRow {
                args_handle: Some(b"h-young"),
                ..call("e1", "c1")
            },
            CallRow {
                args_handle: Some(b"h-old"),
                ..call("e1", "c2")
            },
        ],
        FixtureResolver {
            values: vec![(b"h-young", customer(22)), (b"h-old", customer(41))],
            ..FixtureResolver::default()
        },
    )
    .await;
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM (SELECT call_id, args FROM calls_v1) t \
             WHERE t.args['customer']['age'] >= 30",
        )
        .await
        .expect("derived-table value predicate plans");
    assert_eq!(call_ids(&mut run).await, vec!["c2"]);
    assert_eq!(run.finish().result_state, ResultState::Complete);
    // And the output chain still renders bare values through sort/limit.
    let mut run = fx
        .session
        .execute("SELECT args FROM calls_v1 ORDER BY call_id LIMIT 1")
        .await
        .expect("bare value through sort/limit plans");
    let batch = run.next_batch().await.expect("one batch");
    assert!(
        matches!(
            batch.schema().field(0).data_type(),
            datafusion::arrow::datatypes::DataType::Utf8
        ),
        "output projection renders to text"
    );
}

#[tokio::test]
async fn contradictory_execution_id_equalities_return_zero_rows() {
    // Regression: the provider's pushed-filter fold treated an empty
    // intersection like "no filter yet", so a third equality widened the
    // result back open.
    let fx = fixture(
        vec![call("e1", "c1"), call("e2", "c2")],
        FixtureResolver::default(),
    )
    .await;
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM calls_v1 WHERE execution_id = 'e1' \
             AND execution_id = 'e2' AND execution_id = 'e1'",
        )
        .await
        .expect("plans");
    assert_eq!(call_ids(&mut run).await, Vec::<String>::new());
    assert_eq!(run.finish().result_state, ResultState::Complete);
}

#[tokio::test]
async fn reserved_prefix_in_string_literals_is_legal_data() {
    // Regression: the gatekeeper substring-matched the rendered SQL, so
    // `'__baml_x'` inside a string literal was rejected as reserved.
    let fx = fixture(vec![call("e1", "c1")], FixtureResolver::default()).await;
    let mut run = fx
        .session
        .execute("SELECT call_id FROM calls_v1 WHERE call_id != '__baml_x'")
        .await
        .expect("prefix inside a string literal is data, not an identifier");
    assert_eq!(call_ids(&mut run).await, vec!["c1"]);
    // Outside a literal it stays reserved.
    let err = fx
        .session
        .execute("SELECT __baml_path(args, '[]', 'input') FROM calls_v1")
        .await
        .err()
        .expect("reserved identifier still rejected")
        .0;
    assert_eq!(err.code, QueryErrorCode::InvalidSql);
}

// ── final limit vs value predicates ────────────────────────────────────

#[tokio::test]
async fn final_limit_never_reaches_the_provider_below_a_value_predicate() {
    let rows: Vec<CallRow> = (0..10)
        .map(|i| CallRow {
            args_handle: Some(if i % 2 == 0 { b"h-even" } else { b"h-odd" }),
            ..call("e1", format!("c{i}"))
        })
        .collect();
    let fx = fixture(
        rows,
        FixtureResolver {
            values: vec![(b"h-even", customer(50)), (b"h-odd", customer(20))],
            ..FixtureResolver::default()
        },
    )
    .await;
    // Value-predicate query with LIMIT: the provider must see NO limit —
    // the limit applies only after the exact residual predicate.
    let mut run = fx
        .session
        .execute(
            "SELECT call_id FROM calls_v1 \
             WHERE args['customer']['age'] >= 30 LIMIT 3",
        )
        .await
        .expect("plans");
    let ids = call_ids(&mut run).await;
    assert_eq!(ids.len(), 3);
    assert!(
        ids.iter()
            .all(|id| id.trim_start_matches('c').parse::<u64>().unwrap() % 2 == 0),
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
        .execute("SELECT call_id FROM calls_v1 LIMIT 3")
        .await
        .expect("plans");
    let ids = call_ids(&mut run).await;
    assert_eq!(ids.len(), 3);
    let scans = fx.scans.lock().unwrap().clone();
    assert!(
        scans.contains(&Some(3)),
        "resident-only limits push down: {scans:?}"
    );
}

// ── aliases and snapshot ───────────────────────────────────────────────

#[tokio::test]
async fn unversioned_aliases_are_pinned_to_the_bound_catalog_version() {
    let fx = fixture(vec![call("e1", "c7")], FixtureResolver::default()).await;
    for table in ["calls_v1", "calls", "retained_calls"] {
        let mut run = fx
            .session
            .execute(&format!("SELECT call_id FROM {table}"))
            .await
            .expect("plans");
        assert_eq!(call_ids(&mut run).await, vec!["c7"], "{table}");
        let outcome = run.finish();
        assert_eq!(outcome.snapshot.catalog_version, "v1");
        assert_eq!(outcome.snapshot.generation, "fixture-1");
    }
}

// ── dependency hygiene (backend-neutral core) ──────────────────────────

/// The core must not depend on the engine, the events transport, the
/// CLI, an AWS SDK, or a concrete SQLite/ClickHouse client — pinned
/// against the manifest (scope §3.1).
#[test]
fn core_dependencies_stay_backend_neutral() {
    let manifest = include_str!("../Cargo.toml");
    let allowed = [
        "async-trait",
        "datafusion",
        "futures",
        "serde",
        "serde_json",
        "strsim",
        "thiserror",
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
             contract forbids engine/transport/CLI/AWS/SQLite/ClickHouse \
             coupling; extend the allowlist only with a deliberate decision"
        );
    }
    for forbidden in [
        "bex_events",
        "bex_engine",
        "baml_cli",
        "aws",
        "rusqlite",
        "clickhouse",
    ] {
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
    use datafusion::arrow::array::StringArray;
    let fx = fixture(
        vec![
            CallRow {
                args_handle: Some(b"h-b"),
                ..call("e1", "c2")
            },
            CallRow {
                args_handle: Some(b"h-a"),
                ..call("e1", "c1")
            },
        ],
        FixtureResolver {
            values: vec![(b"h-a", Value::Int(1)), (b"h-b", Value::Int(2))],
            ..FixtureResolver::default()
        },
    )
    .await;
    let mut run = fx
        .session
        .execute("SELECT c.call_id, c.args FROM calls_v1 c ORDER BY c.call_id")
        .await
        .expect("qualified value projection plans");
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
