//! The versioned public logical catalog (D16; catalog v1 freeze).
//!
//! Relations here are logical contracts: name, grain, columns with exact
//! Arrow types and nullability, key/identity scope, availability
//! semantics, and resident-versus-virtual status. Physical sources stay
//! provider-private trusted mappings. Schemas follow
//! `CANONICAL/PROJECT_STUDIO_QUERY_EXAMPLES.md` with the IN-Q1 freeze
//! resolutions (named `args` root, no `call_site_id` until the producer
//! exists, no `process_id`/`engine_id` — `run_id` scopes call identity).

use std::collections::BTreeMap;
use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};

/// The catalog version this crate freezes.
pub const CATALOG_V1: &str = "v1";

/// Field-metadata key marking a virtual BAML value column. The physical
/// column carries an opaque provider handle (Binary); the value itself is
/// hydrated on demand and never resident.
pub const VALUE_META_KEY: &str = "baml.virtual";
pub const VALUE_META_VALUE: &str = "value";
/// Field-metadata key naming the captured role (`args`/`return`/`error`).
pub const VALUE_ROLE_KEY: &str = "baml.role";

/// What one row of a relation represents (grain honesty is part of the
/// public contract: retained counts are never population counts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grain {
    /// One boundary/run.
    Run,
    /// One distinct call-tree location within one run (population-true).
    PopulationCctNode,
    /// One individually retained call (never "all calls").
    RetainedCall,
    /// One immutable grouped evidence-issue summary.
    EvidenceIssue,
    /// One retained exact-evidence region.
    ExactWindow,
    /// One function within one revision (compile-time structure).
    RevisionFunction,
    /// One compiled revision.
    Revision,
    /// One run × call-tree location × model aggregate (provisional).
    LlmPopulation,
    /// One aggregate spawn edge within one run.
    SpawnEdge,
}

/// One column's public contract.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: &'static str,
    pub data_type: DataType,
    pub nullable: bool,
    /// Part of the relation's logical uniqueness key.
    pub key: bool,
    /// A virtual BAML value column (role name). The physical Arrow type
    /// is Binary (opaque handle); selecting or filtering it triggers
    /// budgeted hydration through the ValueResolver.
    pub value_role: Option<&'static str>,
    pub doc: &'static str,
}

/// One relation's public contract.
#[derive(Debug, Clone)]
pub struct RelationDef {
    /// Versioned canonical name (`runs_v1`).
    pub name: &'static str,
    /// Convenience alias bound to this catalog version (`runs`). Saved /
    /// portable queries should use the versioned name; the alias is
    /// pinned to the session's bound catalog version, never "latest".
    pub alias: &'static str,
    pub grain: Grain,
    /// Provisional relations may change shape with a language/runtime
    /// decision (documented per relation) without a catalog major bump.
    pub provisional: bool,
    pub columns: Vec<ColumnDef>,
    pub doc: &'static str,
}

impl RelationDef {
    /// The Arrow schema (virtual value columns appear as Binary handles
    /// with `baml.virtual`/`baml.role` field metadata).
    #[must_use]
    pub fn schema(&self) -> SchemaRef {
        let fields: Vec<Field> = self
            .columns
            .iter()
            .map(|c| {
                let field = Field::new(c.name, c.data_type.clone(), c.nullable);
                match c.value_role {
                    Some(role) => field.with_metadata(
                        BTreeMap::from([
                            (VALUE_META_KEY.to_string(), VALUE_META_VALUE.to_string()),
                            (VALUE_ROLE_KEY.to_string(), role.to_string()),
                        ])
                        .into_iter()
                        .collect(),
                    ),
                    None => field,
                }
            })
            .collect();
        Arc::new(Schema::new(fields))
    }

    #[must_use]
    pub fn key_columns(&self) -> Vec<&'static str> {
        self.columns
            .iter()
            .filter(|c| c.key)
            .map(|c| c.name)
            .collect()
    }

    #[must_use]
    pub fn column(&self, name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// The complete versioned catalog.
#[derive(Debug, Clone)]
pub struct LogicalCatalog {
    pub version: &'static str,
    pub relations: Vec<RelationDef>,
}

impl LogicalCatalog {
    /// Resolve a canonical name or its version-pinned alias.
    #[must_use]
    pub fn relation(&self, name: &str) -> Option<&RelationDef> {
        self.relations
            .iter()
            .find(|r| r.name == name || r.alias == name)
    }
}

// ── column constructors ────────────────────────────────────────────────

fn ts() -> DataType {
    DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into()))
}

fn col(name: &'static str, data_type: DataType, doc: &'static str) -> ColumnDef {
    ColumnDef {
        name,
        data_type,
        nullable: false,
        key: false,
        value_role: None,
        doc,
    }
}

fn nullable(name: &'static str, data_type: DataType, doc: &'static str) -> ColumnDef {
    ColumnDef {
        nullable: true,
        ..col(name, data_type, doc)
    }
}

fn key(name: &'static str, data_type: DataType, doc: &'static str) -> ColumnDef {
    ColumnDef {
        key: true,
        ..col(name, data_type, doc)
    }
}

/// Virtual BAML value column: Binary opaque handle, always nullable at
/// the Arrow level (NULL handle = role not applicable; every other
/// unavailability is typed inside the handle so the resolver can account
/// for it).
fn value(name: &'static str, role: &'static str, doc: &'static str) -> ColumnDef {
    ColumnDef {
        name,
        data_type: DataType::Binary,
        nullable: true,
        key: false,
        value_role: Some(role),
        doc,
    }
}

fn id_list(name: &'static str, doc: &'static str) -> ColumnDef {
    col(
        name,
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, false))),
        doc,
    )
}

fn histogram() -> DataType {
    DataType::FixedSizeList(Arc::new(Field::new("item", DataType::UInt64, false)), 16)
}

// ── catalog v1 ─────────────────────────────────────────────────────────

/// Build the frozen catalog v1.
#[must_use]
pub fn catalog_v1() -> LogicalCatalog {
    LogicalCatalog {
        version: CATALOG_V1,
        relations: vec![
            runs_v1(),
            cct_population_v1(),
            retained_calls_v1(),
            evidence_issues_v1(),
            exact_windows_v1(),
            functions_v1(),
            revisions_v1(),
            llm_population_v1(),
            spawn_edges_v1(),
        ],
    }
}

fn runs_v1() -> RelationDef {
    RelationDef {
        name: "runs_v1",
        alias: "runs",
        grain: Grain::Run,
        provisional: false,
        doc: "One row per program run. Every workflow starts here; run \
              lists, lifecycle/revision filters, error totals, and \
              evidence-health filters read no call or value data.",
        columns: vec![
            key(
                "run_id",
                DataType::Utf8,
                "Stable run identity (boundary id wire form); joins every run-scoped relation.",
            ),
            col("started_at", ts(), "Run begin instant (UTC)."),
            nullable("ended_at", ts(), "Terminal instant; absent while running."),
            col(
                "duration_ns",
                DataType::UInt64,
                "Exact monotonic elapsed (or so-far) time; wall-clock subtraction is not reliably exact.",
            ),
            col(
                "status",
                DataType::Utf8,
                "pending|running|waiting|succeeded|failed|cancelled|panicked|abandoned (IN-Q1-5 mapping).",
            ),
            col(
                "revision_id",
                DataType::Utf8,
                "Exact compiled program identity; joins revisions_v1/functions_v1.",
            ),
            nullable(
                "entry_function_id",
                DataType::UInt32,
                "Root function id within the run's revision, when the entrypoint is a BAML function.",
            ),
            col(
                "entrypoint",
                DataType::Utf8,
                "Readable command/test/function target, including non-function entrypoints.",
            ),
            col(
                "total_calls",
                DataType::UInt64,
                "Population call total (kept resident to avoid scanning call-tree summaries per run-list page).",
            ),
            col(
                "total_errors",
                DataType::UInt64,
                "Population errored-call total.",
            ),
            col(
                "structure_state",
                DataType::Utf8,
                "complete|incomplete|pending|lost — execution success does not prove structural completeness.",
            ),
            col(
                "value_state",
                DataType::Utf8,
                "complete|partial|pending|not_captured|lost — missing values must not look like ordinary NULL.",
            ),
            col(
                "integrity_state",
                DataType::Utf8,
                "verified|unverified|corrupt|conflicting.",
            ),
            col(
                "projection_state",
                DataType::Utf8,
                "pending|active|delayed|failed|rebuilding — local providers report active.",
            ),
            col(
                "retention_state",
                DataType::Utf8,
                "retained|partially_retained|erased.",
            ),
        ],
    }
}

fn cct_population_v1() -> RelationDef {
    RelationDef {
        name: "cct_population_v1",
        alias: "cct_population",
        grain: Grain::PopulationCctNode,
        provisional: false,
        doc: "One row per distinct call-tree location within one run — \
              population-true aggregates (every runtime call contributes); \
              never one row per invocation.",
        columns: vec![
            key("run_id", DataType::Utf8, "Scopes the location to one run."),
            key(
                "node_id",
                DataType::UInt32,
                "Stable call-path identity within the run's sealed fold.",
            ),
            nullable(
                "parent_node_id",
                DataType::UInt32,
                "Tree parent; absent for the root.",
            ),
            col(
                "depth",
                DataType::UInt32,
                "Depth from the run root (cheap indentation/filters).",
            ),
            col(
                "function_id",
                DataType::UInt32,
                "Function identity within the run's revision.",
            ),
            col(
                "revision_id",
                DataType::Utf8,
                "Repeated for hot cross-run grouping without a runs join (deliberate duplication).",
            ),
            nullable(
                "definition_key",
                DataType::Utf8,
                "Stable logical function identity across revisions; absent for synthetic functions.",
            ),
            nullable(
                "local_definition_hash",
                DataType::FixedSizeBinary(32),
                "This function's own compiled signature/bytecode hash — NOT a dependency-closure version.",
            ),
            col(
                "fqn",
                DataType::Utf8,
                "Fully qualified function name for display/grouping.",
            ),
            col(
                "calls_started",
                DataType::UInt64,
                "Total entries (rate/mean denominator).",
            ),
            col("calls_succeeded", DataType::UInt64, "Terminal successes."),
            col("calls_errored", DataType::UInt64, "Terminal errors."),
            col(
                "calls_cancelled",
                DataType::UInt64,
                "Terminal cancellations (distinct from failure).",
            ),
            col(
                "calls_exited",
                DataType::UInt64,
                "Other explicit terminal exits.",
            ),
            col(
                "inclusive_ns",
                DataType::UInt64,
                "Function plus nested calls.",
            ),
            col("self_ns", DataType::UInt64, "Direct execution only."),
            col("await_ns", DataType::UInt64, "Suspended/waiting time."),
            nullable(
                "duration_histogram",
                histogram(),
                "16 fixed catalog-owned buckets (4x stride from 1 microsecond); explicit lower bounds after declared saturation.",
            ),
        ],
    }
}

fn retained_calls_v1() -> RelationDef {
    RelationDef {
        name: "retained_calls_v1",
        alias: "retained_calls",
        grain: Grain::RetainedCall,
        provisional: false,
        doc: "One row per individually retained call. Bounded by capture/\
              retention policy — never implies it contains all calls; \
              population totals live in cct_population_v1. args/return/\
              error are virtual value fields hydrated on demand.",
        columns: vec![
            key(
                "run_id",
                DataType::Utf8,
                "Scopes the invocation to one run (run_id + call_id is the call's identity scope; IN-Q1 removed process/engine ids).",
            ),
            key(
                "call_id",
                DataType::UInt64,
                "Exact invocation identity for lookup and causal links.",
            ),
            nullable(
                "parent_call_id",
                DataType::UInt64,
                "Exact parentage when known; the parent itself may not be retained.",
            ),
            nullable(
                "node_id",
                DataType::UInt32,
                "Joins the retained example to its cct_population_v1 summary.",
            ),
            col(
                "thread_id",
                DataType::UInt64,
                "Logical execution thread (per-thread ordering/concurrency).",
            ),
            nullable(
                "definition_key",
                DataType::Utf8,
                "Duplicated logical-function filter; other function metadata joins through the revision.",
            ),
            nullable(
                "started_at",
                ts(),
                "Orders retained calls on the run timeline.",
            ),
            nullable("ended_at", ts(), "Absent while running."),
            nullable(
                "duration_ns",
                DataType::UInt64,
                "Exact monotonic (or so-far) duration.",
            ),
            col(
                "status",
                DataType::Utf8,
                "pending|running|waiting|succeeded|failed|cancelled|panicked|abandoned.",
            ),
            id_list(
                "retention_reasons",
                "Why this call exists in a selective table (policy|incident|promotion|explicit).",
            ),
            id_list(
                "exact_window_ids",
                "Every retained incident window containing this call.",
            ),
            id_list(
                "evidence_ids",
                "Joinable logical evidence identities (never S3 keys, CIDs, or byte ranges).",
            ),
            nullable(
                "capture_policy_version",
                DataType::UInt32,
                "Which capture rules decided whether values should exist.",
            ),
            col(
                "args_state",
                DataType::Utf8,
                "available|pending|not_captured|omitted|redacted|lost|truncated|corrupt|unsupported.",
            ),
            col(
                "return_state",
                DataType::Utf8,
                "available|pending|not_applicable|not_captured|omitted|redacted|lost|truncated|corrupt|unsupported.",
            ),
            col(
                "error_state",
                DataType::Utf8,
                "available|pending|not_applicable|not_captured|omitted|redacted|lost|truncated|corrupt|unsupported.",
            ),
            value(
                "args",
                "args",
                "Virtual: the named-argument object (IN-Q1-1) hydrated from canonical evidence; args['name'] selects a parameter.",
            ),
            value("return", "return", "Virtual: the captured output value."),
            value("error", "error", "Virtual: the captured error value."),
        ],
    }
}

fn evidence_issues_v1() -> RelationDef {
    RelationDef {
        name: "evidence_issues_v1",
        alias: "evidence_issues",
        grain: Grain::EvidenceIssue,
        provisional: false,
        doc: "One immutable grouped issue summary per sealed source scope \
              and kind/reason — the explicit account of missing or \
              degraded evidence. Never one row per affected event.",
        columns: vec![
            key(
                "issue_id",
                DataType::Utf8,
                "Stable identity for one sealed grouped summary.",
            ),
            nullable(
                "run_id",
                DataType::Utf8,
                "User-visible run attribution when known.",
            ),
            nullable(
                "session_id",
                DataType::Utf8,
                "Runtime scope for issues outside a single run binding.",
            ),
            nullable(
                "evidence_id",
                DataType::Utf8,
                "The retained evidence whose completeness/integrity is affected.",
            ),
            col(
                "source",
                DataType::Utf8,
                "profiler|value_capture|uploader|projector|retention.",
            ),
            col(
                "kind",
                DataType::Utf8,
                "Which evidence class is incomplete (structure|values|events|...).",
            ),
            col(
                "reason",
                DataType::Utf8,
                "Typed cause (groupable without parsing messages).",
            ),
            col(
                "count",
                DataType::UInt64,
                "Affected facts, compressed before insertion.",
            ),
            col("first_seen_at", ts(), "When the grouped issue began."),
            col(
                "last_seen_at",
                ts(),
                "Observed extent of the grouped issue.",
            ),
            nullable(
                "policy_version",
                DataType::UInt32,
                "Explains policy-caused omissions; absent for integrity/infrastructure failures.",
            ),
        ],
    }
}

fn exact_windows_v1() -> RelationDef {
    RelationDef {
        name: "exact_windows_v1",
        alias: "exact_windows",
        grain: Grain::ExactWindow,
        provisional: false,
        doc: "The searchable ledger of preserved exact-event regions \
              (flight dumps, recent rings, raw, explicit captures). Not \
              the tape itself: detailed events stay in the evidence body.",
        columns: vec![
            key(
                "run_id",
                DataType::Utf8,
                "Scopes the retained region to the run under investigation.",
            ),
            key(
                "window_id",
                DataType::Utf8,
                "Stable identity for links from calls and spawns.",
            ),
            col(
                "session_id",
                DataType::Utf8,
                "Profiler-session scope (supports recovery before run binding).",
            ),
            col(
                "source",
                DataType::Utf8,
                "recent_ring|flight_dump|raw|explicit.",
            ),
            col("trigger", DataType::Utf8, "error|manual|policy|other."),
            nullable(
                "trigger_node_id",
                DataType::UInt32,
                "The aggregate location that caused retention.",
            ),
            nullable(
                "trigger_call_id",
                DataType::UInt64,
                "The exact triggering call when retained.",
            ),
            col("started_at", ts(), "Start of the retained event interval."),
            col("ended_at", ts(), "End of the retained event interval."),
            col(
                "event_count",
                DataType::UInt64,
                "Evidence size without opening the bytes.",
            ),
            col(
                "evidence_state",
                DataType::Utf8,
                "available|incomplete|pending|lost|corrupt.",
            ),
            id_list(
                "incomplete_reasons",
                "evicted|budget_exhausted|truncated|unsupported — every known reason a present window is incomplete.",
            ),
            col(
                "evidence_id",
                DataType::Utf8,
                "Stable public evidence identity; provider-private mapping locates the bytes.",
            ),
        ],
    }
}

fn functions_v1() -> RelationDef {
    RelationDef {
        name: "functions_v1",
        alias: "functions",
        grain: Grain::RevisionFunction,
        provisional: false,
        doc: "Identity dictionary: one row per function within one \
              revision. call_site columns are deliberately absent until \
              the dictionary producer emits call sites (additive later).",
        columns: vec![
            key(
                "revision_id",
                DataType::Utf8,
                "Scopes revision-local function ids.",
            ),
            key(
                "function_id",
                DataType::UInt32,
                "Compact runtime identity emitted in call evidence.",
            ),
            nullable(
                "definition_key",
                DataType::Utf8,
                "Logical identity across revisions; a rename changes it; absent for synthetic/internal.",
            ),
            nullable(
                "local_definition_hash",
                DataType::FixedSizeBinary(32),
                "This function's own compiled signature/bytecode; not a dependency-closure version.",
            ),
            col("fqn", DataType::Utf8, "Unambiguous user-facing name."),
            col(
                "display_name",
                DataType::Utf8,
                "Concise label for tree/list views.",
            ),
            nullable(
                "source_path",
                DataType::Utf8,
                "Project-relative definition file; absent for functions without source.",
            ),
            nullable(
                "source_start",
                DataType::UInt32,
                "Definition start offset for editor navigation.",
            ),
            nullable("source_end", DataType::UInt32, "Definition end offset."),
            nullable(
                "source_line",
                DataType::UInt32,
                "1-based start line for cheap display.",
            ),
            col(
                "kind",
                DataType::Utf8,
                "bytecode|native|system — execution kinds whose timing must not be conflated.",
            ),
            col(
                "origin",
                DataType::Utf8,
                "user|companion|internal|builtin|generated.",
            ),
            col(
                "capture_inputs",
                DataType::Utf8,
                "disabled|auto|enabled — effective input-capture intent.",
            ),
            col("capture_output", DataType::Utf8, "disabled|auto|enabled."),
            col("capture_error", DataType::Utf8, "disabled|auto|enabled."),
            col(
                "promote_on_error",
                DataType::Utf8,
                "disabled|auto|enabled (wiring deferred; intent recorded).",
            ),
        ],
    }
}

fn revisions_v1() -> RelationDef {
    RelationDef {
        name: "revisions_v1",
        alias: "revisions",
        grain: Grain::Revision,
        provisional: false,
        doc: "One row per compiled revision: the exact program identity \
              that scopes function/call ids and explains behavior.",
        columns: vec![
            key(
                "revision_id",
                DataType::Utf8,
                "BLAKE3 over source snapshot + compiler identity + behavior-affecting options.",
            ),
            col(
                "source_snapshot_id",
                DataType::Utf8,
                "Exact source snapshot users need to inspect.",
            ),
            col(
                "compiler_id",
                DataType::Utf8,
                "Which compiler produced the artifact.",
            ),
            col(
                "capture_policy_version",
                DataType::UInt32,
                "Policy semantics for decoded capture fields.",
            ),
            col(
                "identity_state",
                DataType::Utf8,
                "verified|fallback_legacy.",
            ),
            col(
                "first_seen_at",
                ts(),
                "Revision discovery/ordering when source metadata is unavailable.",
            ),
        ],
    }
}

fn llm_population_v1() -> RelationDef {
    RelationDef {
        name: "llm_population_v1",
        alias: "llm_population",
        grain: Grain::LlmPopulation,
        provisional: true,
        doc: "PROVISIONAL (pending the LLM-model rework): one row per run \
              x call-tree location x model. Model identity only — current \
              evidence exposes no separate provider identity (IN-Q1-7).",
        columns: vec![
            key("run_id", DataType::Utf8, "Run scope for token accounting."),
            key(
                "node_id",
                DataType::UInt32,
                "The call-tree location that caused the LLM activity.",
            ),
            key(
                "model",
                DataType::Utf8,
                "Model users selected (interned model name).",
            ),
            col("llm_calls", DataType::UInt64, "Invocation denominator."),
            col(
                "token_state",
                DataType::Utf8,
                "available|partial|unavailable — zero usage stays distinct from unmeasured.",
            ),
            nullable(
                "input_tokens",
                DataType::UInt64,
                "Input usage when measured.",
            ),
            nullable(
                "output_tokens",
                DataType::UInt64,
                "Output usage when measured.",
            ),
            col(
                "provider_errors",
                DataType::UInt64,
                "Failures the provider returned.",
            ),
            col(
                "parse_errors",
                DataType::UInt64,
                "Responses that arrived but did not parse.",
            ),
        ],
    }
}

fn spawn_edges_v1() -> RelationDef {
    RelationDef {
        name: "spawn_edges_v1",
        alias: "spawn_edges",
        grain: Grain::SpawnEdge,
        provisional: false,
        doc: "One row per unique parent-location/child-function spawn \
              relationship within one run (aggregate fan-out; retained \
              instances are a separate future relation).",
        columns: vec![
            key("run_id", DataType::Utf8, "Run scope."),
            key(
                "edge_id",
                DataType::UInt32,
                "Stable join key for retained examples.",
            ),
            col(
                "parent_node_id",
                DataType::UInt32,
                "The call-tree location that initiated child work.",
            ),
            col(
                "child_function_id",
                DataType::UInt32,
                "The spawned function (via the run's revision).",
            ),
            col(
                "spawned",
                DataType::UInt64,
                "Total fan-out (completion/error denominator).",
            ),
            col("completed", DataType::UInt64, "Successful completions."),
            col("errored", DataType::UInt64, "Failed child work."),
            col(
                "cancelled",
                DataType::UInt64,
                "Cancelled child work (distinct from errors).",
            ),
            nullable(
                "running_ns",
                DataType::UInt64,
                "Child execution time; absent until its accounting is exact.",
            ),
            nullable(
                "awaiting_ns",
                DataType::UInt64,
                "Parent wait time; absent until its accounting is exact.",
            ),
            col(
                "retained_instances",
                DataType::UInt64,
                "How many exact spawn examples are inspectable.",
            ),
            col(
                "instances_dropped",
                DataType::UInt64,
                "Prevents mistaking the selective instance set for complete history.",
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_v1_relations_and_keys_are_frozen() {
        let catalog = catalog_v1();
        let names: Vec<&str> = catalog.relations.iter().map(|r| r.name).collect();
        assert_eq!(
            names,
            vec![
                "runs_v1",
                "cct_population_v1",
                "retained_calls_v1",
                "evidence_issues_v1",
                "exact_windows_v1",
                "functions_v1",
                "revisions_v1",
                "llm_population_v1",
                "spawn_edges_v1",
            ],
        );
        let runs = catalog.relation("runs_v1").unwrap();
        assert_eq!(runs.key_columns(), vec!["run_id"]);
        let cct = catalog.relation("cct_population").unwrap();
        assert_eq!(
            cct.name, "cct_population_v1",
            "alias resolves, version pinned"
        );
        assert_eq!(cct.key_columns(), vec!["run_id", "node_id"]);
        let calls = catalog.relation("retained_calls_v1").unwrap();
        assert_eq!(calls.key_columns(), vec!["run_id", "call_id"]);
    }

    #[test]
    fn virtual_value_columns_are_binary_handles_with_metadata() {
        let calls = catalog_v1();
        let calls = calls.relation("retained_calls_v1").unwrap();
        let schema = calls.schema();
        for role in ["args", "return", "error"] {
            let field = schema.field_with_name(role).unwrap();
            assert_eq!(field.data_type(), &DataType::Binary);
            assert!(field.is_nullable());
            assert_eq!(
                field.metadata().get(VALUE_META_KEY).map(String::as_str),
                Some(VALUE_META_VALUE),
            );
            assert_eq!(
                field.metadata().get(VALUE_ROLE_KEY).map(String::as_str),
                Some(role),
            );
        }
        // Exactly the three roles are virtual; everything else is resident.
        let virtuals = calls
            .columns
            .iter()
            .filter(|c| c.value_role.is_some())
            .count();
        assert_eq!(virtuals, 3);
    }

    #[test]
    fn no_call_site_or_process_engine_columns_before_their_producers() {
        let catalog = catalog_v1();
        assert!(catalog.relation("call_sites_v1").is_none());
        let calls = catalog.relation("retained_calls_v1").unwrap();
        for absent in ["call_site_id", "process_id", "engine_id"] {
            assert!(
                calls.column(absent).is_none(),
                "{absent} must not exist yet"
            );
        }
    }

    /// The frozen column golden: any addition, removal, reorder, or
    /// retype of a catalog-v1 column must be a deliberate edit HERE.
    #[test]
    fn catalog_v1_column_golden() {
        let catalog = catalog_v1();
        let render = |name: &str| -> Vec<String> {
            catalog
                .relation(name)
                .unwrap()
                .columns
                .iter()
                .map(|c| {
                    format!(
                        "{}:{:?}{}{}",
                        c.name,
                        c.data_type,
                        if c.nullable { "?" } else { "" },
                        if c.value_role.is_some() {
                            ":virtual"
                        } else {
                            ""
                        },
                    )
                })
                .collect()
        };
        insta_lines(
            render("runs_v1"),
            &[
                "run_id:Utf8",
                "started_at:Timestamp(Nanosecond, Some(\"UTC\"))",
                "ended_at:Timestamp(Nanosecond, Some(\"UTC\"))?",
                "duration_ns:UInt64",
                "status:Utf8",
                "revision_id:Utf8",
                "entry_function_id:UInt32?",
                "entrypoint:Utf8",
                "total_calls:UInt64",
                "total_errors:UInt64",
                "structure_state:Utf8",
                "value_state:Utf8",
                "integrity_state:Utf8",
                "projection_state:Utf8",
                "retention_state:Utf8",
            ],
        );
        insta_lines(
            render("retained_calls_v1"),
            &[
                "run_id:Utf8",
                "call_id:UInt64",
                "parent_call_id:UInt64?",
                "node_id:UInt32?",
                "thread_id:UInt64",
                "definition_key:Utf8?",
                "started_at:Timestamp(Nanosecond, Some(\"UTC\"))?",
                "ended_at:Timestamp(Nanosecond, Some(\"UTC\"))?",
                "duration_ns:UInt64?",
                "status:Utf8",
                "retention_reasons:List(Field { data_type: Utf8 })",
                "exact_window_ids:List(Field { data_type: Utf8 })",
                "evidence_ids:List(Field { data_type: Utf8 })",
                "capture_policy_version:UInt32?",
                "args_state:Utf8",
                "return_state:Utf8",
                "error_state:Utf8",
                "args:Binary?:virtual",
                "return:Binary?:virtual",
                "error:Binary?:virtual",
            ],
        );
        // Column-name goldens for the rest (types pinned by construction
        // helpers shared with the two full goldens above).
        let names = |name: &str| -> Vec<&'static str> {
            catalog
                .relation(name)
                .unwrap()
                .columns
                .iter()
                .map(|c| c.name)
                .collect()
        };
        assert_eq!(
            names("cct_population_v1"),
            vec![
                "run_id",
                "node_id",
                "parent_node_id",
                "depth",
                "function_id",
                "revision_id",
                "definition_key",
                "local_definition_hash",
                "fqn",
                "calls_started",
                "calls_succeeded",
                "calls_errored",
                "calls_cancelled",
                "calls_exited",
                "inclusive_ns",
                "self_ns",
                "await_ns",
                "duration_histogram",
            ]
        );
        assert_eq!(
            names("evidence_issues_v1"),
            vec![
                "issue_id",
                "run_id",
                "session_id",
                "evidence_id",
                "source",
                "kind",
                "reason",
                "count",
                "first_seen_at",
                "last_seen_at",
                "policy_version",
            ]
        );
        assert_eq!(
            names("exact_windows_v1"),
            vec![
                "run_id",
                "window_id",
                "session_id",
                "source",
                "trigger",
                "trigger_node_id",
                "trigger_call_id",
                "started_at",
                "ended_at",
                "event_count",
                "evidence_state",
                "incomplete_reasons",
                "evidence_id",
            ]
        );
        assert_eq!(
            names("functions_v1"),
            vec![
                "revision_id",
                "function_id",
                "definition_key",
                "local_definition_hash",
                "fqn",
                "display_name",
                "source_path",
                "source_start",
                "source_end",
                "source_line",
                "kind",
                "origin",
                "capture_inputs",
                "capture_output",
                "capture_error",
                "promote_on_error",
            ]
        );
        assert_eq!(
            names("revisions_v1"),
            vec![
                "revision_id",
                "source_snapshot_id",
                "compiler_id",
                "capture_policy_version",
                "identity_state",
                "first_seen_at",
            ]
        );
        assert_eq!(
            names("llm_population_v1"),
            vec![
                "run_id",
                "node_id",
                "model",
                "llm_calls",
                "token_state",
                "input_tokens",
                "output_tokens",
                "provider_errors",
                "parse_errors",
            ]
        );
        assert_eq!(
            names("spawn_edges_v1"),
            vec![
                "run_id",
                "edge_id",
                "parent_node_id",
                "child_function_id",
                "spawned",
                "completed",
                "errored",
                "cancelled",
                "running_ns",
                "awaiting_ns",
                "retained_instances",
                "instances_dropped",
            ]
        );
    }

    fn insta_lines(actual: Vec<String>, expected: &[&str]) {
        let rendered: Vec<&str> = actual.iter().map(String::as_str).collect();
        assert_eq!(rendered, expected);
    }

    #[test]
    fn every_column_documents_itself() {
        for relation in catalog_v1().relations {
            assert!(!relation.doc.is_empty(), "{} has no doc", relation.name);
            for column in &relation.columns {
                assert!(
                    !column.doc.is_empty(),
                    "{}.{} has no doc",
                    relation.name,
                    column.name
                );
            }
        }
    }
}
